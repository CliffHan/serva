use axum::{
    body::{Full, StreamBody},
    extract::Path as AxumPath,
    http::{header, StatusCode},
    response::{AppendHeaders, IntoResponse, Redirect, Response},
    routing::get,
    Extension, Router,
};
use http::{
    header::{
        ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE,
    },
    HeaderMap,
};
use log::{debug, trace};
use std::{
    error::Error,
    io::SeekFrom,
    ops::RangeInclusive,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use tower_http::cors::CorsLayer;

use crate::data::{get_valid_joined_path, Asset, ServerInfo};

// type AnyError = anyhow::Error;
type ExtConfig = Extension<Arc<Config>>;

#[derive(Debug)]
pub struct Config {
    root: PathBuf,
    prefix: String,
    allow_cors: bool,
    allow_download: bool,
}

impl From<&ServerInfo> for Config {
    fn from(server_info: &ServerInfo) -> Self {
        Config {
            root: server_info.root.clone(),
            prefix: server_info.prefix.clone(),
            allow_cors: server_info.arg_allow_cors,
            allow_download: server_info.arg_allow_download,
        }
    }
}

macro_rules! unwrap_result_or_return {
    ($result: expr) => {
        match $result {
            Ok(r) => r,
            Err(e) => {
                trace!("Error: {:?}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };
    ($result: expr, $resp: expr) => {
        match $result {
            Ok(r) => r,
            Err(e) => {
                trace!("Error: {:?}", e);
                return $resp.into_response();
            }
        }
    };
}

macro_rules! unwrap_option_or_return {
    ($option: expr) => {
        match $option {
            Some(r) => r,
            None => {
                trace!("Failed to unwrap Option");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    };
}

fn build_response_from_result(result: Result<impl IntoResponse, impl Error>) -> Response {
    match result {
        Ok(response) => response.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn build_range_error_response(size: u64) -> Response {
    let append_headers = AppendHeaders([(CONTENT_RANGE, format!("bytes */{}", size))]);
    (append_headers, StatusCode::RANGE_NOT_SATISFIABLE).into_response()
}

async fn build_range_response(file_path: &Path, range: &RangeInclusive<u64>) -> Response {
    let mut file = match tokio::fs::File::open(file_path).await {
        Ok(file) => file,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    // build body from range
    let start = *range.start();
    if (file.seek(SeekFrom::Start(start)).await).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let end = *range.end();
    let size = end - start + 1;
    let body = StreamBody::new(ReaderStream::new(file.take(size)));
    let mime = mime_guess::from_path(file_path).first_or_octet_stream();
    let headers = AppendHeaders([
        (CONTENT_TYPE, mime.to_string()),
        (ACCEPT_RANGES, "bytes".to_string()),
        (CONTENT_RANGE, format!("bytes {}-{}/{}", start, end, size)),
        (CONTENT_LENGTH, size.to_string()),
    ]);
    trace!(
        "build_range_response(), start={}, end={}, size={}",
        start,
        end,
        size
    );
    (StatusCode::PARTIAL_CONTENT, headers, body).into_response()
}

async fn serve_embedded_files(path: &Path) -> Response {
    debug!("serve_embedded_files(), path={:?}", &path);
    let file_path = unwrap_option_or_return!(path.to_str());

    // when gzip exists, return gzip directly
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let builder = Response::builder().header(header::CONTENT_TYPE, mime.as_ref());
    if let Some(gzip) = Asset::get(&format!("{}.gz", file_path)) {
        trace!("serve_embedded_files(), return gzip data directly");
        let builder = builder.header(header::CONTENT_ENCODING, "gzip");
        return build_response_from_result(builder.body(Full::from(gzip.data)));
    }

    // when file exists, return file data
    if let Some(file) = Asset::get(file_path) {
        trace!("serve_embedded_files(), return file data");
        return build_response_from_result(builder.body(Full::from(file.data)));
    }

    // return NOT_FOUND when file not exists
    trace!("serve_embedded_files(), NOT_FOUND");
    StatusCode::NOT_FOUND.into_response()
}

async fn serve_fs_files(path: &Path, headers: HeaderMap, config: ExtConfig) -> Response {
    debug!("serve_fs_files(), path={:?}", path);
    trace!("serve_fs_files(), headers={:?}", headers);

    // join path to get the full path of fs file
    let full_path = unwrap_result_or_return!(
        get_valid_joined_path(&config.root, path),
        StatusCode::NOT_ACCEPTABLE
    );
    trace!("serve_fs_files(), full_path={:?}", full_path);

    // handle range error
    let length = unwrap_result_or_return!(full_path.metadata()).len();
    trace!("serve_fs_files(), file size={}", length);
    let parse_range_result_option = headers
        .get(RANGE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            http_range_header::parse_range_header(value)
                .and_then(|first_pass| first_pass.validate(length))
        });
    match parse_range_result_option {
        Some(Ok(ranges)) => {
            let only_one_range = ranges.len() == 1;
            match (ranges.first(), only_one_range) {
                (Some(range), true) => {
                    return build_range_response(&full_path, range).await;
                }
                _ => {
                    return build_range_error_response(length);
                }
            }
        }
        Some(Err(_)) => {
            return build_range_error_response(length);
        }
        None => {} // return full body
    };
    // read file and setup response as stream, refer to https://github.com/tokio-rs/axum/discussions/608
    let file = match tokio::fs::File::open(&full_path).await {
        Ok(file) => file,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let mime = mime_guess::from_path(&full_path).first_or_octet_stream();
    let disposition = format!(
        "attachment; filename={:?}",
        full_path.file_name().unwrap_or_default()
    );
    trace!("disposition={}", disposition);
    let body = StreamBody::new(ReaderStream::new(file));
    let headers = AppendHeaders([
        (CONTENT_TYPE, mime.to_string()),
        (CONTENT_DISPOSITION, disposition),
        (CONTENT_LENGTH, length.to_string()),
    ]);
    (headers, body).into_response()
}

async fn serve_files(
    AxumPath(path): AxumPath<String>,
    headers: HeaderMap,
    config: ExtConfig,
) -> Response {
    trace!("serve_files(), path=={:?}", path);
    let url_path = PathBuf::from(path);

    // only test file path with prefix when download is allowed
    if config.allow_download {
        // when path in url starts with prefix, return corresponding fs file
        if let Ok(fs_file_path) = url_path.strip_prefix(&config.prefix) {
            if fs_file_path.as_os_str().is_empty() {
                // no fs file path specified means file not found
                trace!("serve_files(), prefix only path, return 404");
                return StatusCode::NOT_FOUND.into_response();
            }
            return serve_fs_files(fs_file_path, headers, config).await;
        }
    }

    // strip first '/' and return embedded file as normal
    if let Ok(embedded_file_path) = url_path.strip_prefix("/") {
        if embedded_file_path.as_os_str().is_empty() {
            // redirect to /index.html when no embedded file path specified
            trace!("serve_files(), empty path, redirect to /index.html");
            return Redirect::permanent("/index.html").into_response();
        }
        return serve_embedded_files(embedded_file_path).await;
    }

    // shouldn't be here, but who knows
    StatusCode::BAD_REQUEST.into_response()
}

pub fn get_serve_file_service(server_info: &ServerInfo) -> Router<hyper::Body> {
    let config = Config::from(server_info);
    let allow_cors = config.allow_cors;
    let mut app = Router::new()
        .route("/*path", get(serve_files))
        .layer(Extension(Arc::new(config)));
    if allow_cors {
        app = app.layer(CorsLayer::permissive());
    }
    app
}
