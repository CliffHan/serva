use crate::data::{get_valid_joined_path, ServerInfo};
use anyhow::anyhow;
use log::{debug, trace};
use proto::{
    manage_dir_or_file_request::Operation,
    serva_manager_server::{ServaManager, ServaManagerServer},
    Address, Directory, File, GetConfigRequest, GetConfigResponse, ListDirRequest, ListDirResponse,
    ManageDirOrFileRequest, ManageDirOrFileResponse, Permission, UploadFileChunkRequest,
    UploadFileChunkResponse,
};
#[cfg(not(target_os = "windows"))]
use std::os::unix::prelude::FileExt;
#[cfg(target_os = "windows")]
use std::os::windows::fs::FileExt;
use std::{
    fmt::Debug,
    fs::{read_dir, remove_file},
    net::IpAddr,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tonic::{Code, Request, Response, Status};

type AnyError = anyhow::Error;
type DirEntriesTuple = (Vec<Directory>, Vec<File>);
type TonicListDirReq = Request<ListDirRequest>;
type TonicListDirResp = Response<ListDirResponse>;
type TonicGetCfgReq = Request<GetConfigRequest>;
type TonicGetCfgResp = Response<GetConfigResponse>;
type TonicUploadFileChunkReq = Request<UploadFileChunkRequest>;
type TonicUploadFileChunkResp = Response<UploadFileChunkResponse>;
type TonicManageDirOrFileReq = Request<ManageDirOrFileRequest>;
type TonicManageDirOrFileResp = Response<ManageDirOrFileResponse>;
type ServaManagerServerImpl = ServaManagerServer<ServaManagerServiceImpl>;

pub mod proto {
    include!("generated/api.rs");
}

const UPLOAD_FILE_SUFFIX: &str = "uploading";

#[derive(Debug)]
struct Config {
    root_path: PathBuf,
    root_relative: String,
    root_absolute: String,
    prefix: String,
    available_ip: Vec<IpAddr>,
    port: u16,
    permission: Permission,
    allow_upload: bool,
    allow_manage: bool,
}

impl From<&ServerInfo> for Config {
    fn from(server_info: &ServerInfo) -> Self {
        let permission = Permission {
            create: server_info.arg_allow_manage,
            copy: server_info.arg_allow_manage,
            r#move: server_info.arg_allow_manage,
            delete: server_info.arg_allow_manage,
            rename: server_info.arg_allow_manage,
            upload: server_info.arg_allow_upload,
            download: server_info.arg_allow_download,
        };
        Config {
            root_path: server_info.root.clone(),
            root_relative: server_info.arg_path.clone(),
            root_absolute: server_info.root_canonical.clone(),
            prefix: server_info.prefix.clone(),
            available_ip: server_info.available_ip.clone(),
            port: server_info.arg_port,
            permission,
            allow_upload: server_info.arg_allow_upload,
            allow_manage: server_info.arg_allow_manage,
        }
    }
}

pub struct ServaManagerServiceImpl {
    config: Config,
}

fn get_timestamp_in_ms(time: SystemTime) -> Result<i64, AnyError> {
    // convert time to timestamp in milliseconds
    Ok(time.duration_since(UNIX_EPOCH)?.as_millis() as i64)
}

fn validate_name(name: &str) -> Result<(), AnyError> {
    if name.contains('/') || name.contains('\\') {
        return Err(anyhow!("Name contains invalid character: {}", name));
    }
    Ok(())
}

fn get_upload_path_names(
    root: &Path,
    dir_path: &str,
    file_name: &str,
) -> Result<(PathBuf, PathBuf, PathBuf), AnyError> {
    // get valid full_path for directory only
    let full_path = get_valid_joined_path(root, dir_path)?;
    // validate file_name
    let _ = validate_name(file_name)?;
    // get the full path name of the file to be uploaded
    let target_path_name = full_path.join(file_name);
    // get the full path name of the temporary file to be used
    let temp_path_name = full_path.join(format!("{}.{}", file_name, UPLOAD_FILE_SUFFIX));
    Ok((full_path, target_path_name, temp_path_name))
}

fn write_file(target: &Path, data: &Vec<u8>) -> Result<(), AnyError> {
    Ok(std::fs::write(target, data)?)
}

fn write_first_chunk(
    target: &Path,
    temp: &Path,
    data: &Vec<u8>,
    file_size: u64,
) -> Result<(), AnyError> {
    trace!("write_first_chunk()");
    // write an empty target file as a placeholder
    std::fs::write(target, &[])?;
    write_chunk(temp, data, 0, file_size, true)?;
    Ok(())
}

fn write_chunk(
    temp: &Path,
    data: &Vec<u8>,
    offset: u64,
    file_size: u64,
    new_file: bool,
) -> Result<(), AnyError> {
    trace!("write_chunk()");
    let data_size = data.len();
    if (offset + data_size as u64) > file_size {
        return Err(anyhow!(
            "data exceeds file size, offset={}, data_size={}, file_size={}",
            offset,
            data_size,
            file_size
        ));
    }
    let temp_file = match new_file {
        true => std::fs::File::create(temp)?,
        false => std::fs::OpenOptions::new().write(true).open(temp)?,
    };
    #[cfg(target_os = "windows")]
    temp_file.seek_write(data, offset)?;
    #[cfg(not(target_os = "windows"))]
    temp_file.write_all_at(data, offset)?;
    temp_file.sync_all()?;
    Ok(())
}

fn write_last_chunk(
    target: &Path,
    temp: &Path,
    data: &Vec<u8>,
    offset: u64,
    file_size: u64,
) -> Result<(), AnyError> {
    trace!("write_last_chunk()");
    write_chunk(temp, data, offset, file_size, false)?;
    std::fs::remove_file(target)?;
    std::fs::rename(temp, target)?;
    Ok(())
}

fn remove_files(target_path_name: &Path, temp_path_name: &Path) -> Result<(), AnyError> {
    if target_path_name.exists() {
        remove_file(target_path_name)?;
    }
    if temp_path_name.exists() {
        remove_file(temp_path_name)?;
    }
    Ok(())
}

impl ServaManagerServiceImpl {
    fn get_stripped_path_string(&self, path: &Path) -> Result<String, AnyError> {
        // strip root from path
        let stripped_path = path.strip_prefix(&self.config.root_path)?;
        let path = stripped_path
            .to_str()
            .ok_or_else(|| anyhow!("stripped_path.to_str() failed"))?;
        Ok(path.to_string())
    }

    fn get_dir_entries(&self, dir: &str) -> Result<DirEntriesTuple, AnyError> {
        // get valid full_path
        let full_path = get_valid_joined_path(&self.config.root_path, dir)?;
        trace!("get_dir_entries(), full_path={:?}", &full_path);
        // read entries from path, fill metadata to result
        let entries = read_dir(full_path)?;
        let mut directories = vec![];
        let mut files = vec![];
        for entry in entries {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let entry_path = entry.path();
            trace!("entry_path={:?}", &entry_path,);
            let stripped_path = self.get_stripped_path_string(&entry_path)?;
            trace!("stripped_path={}", &stripped_path);
            let modified_timestamp_in_ms = get_timestamp_in_ms(metadata.modified()?)?;
            // only handle dir and file here, skip symlinks
            match (metadata.is_dir(), metadata.is_file()) {
                (true, false) => directories.push(Directory {
                    path: stripped_path,
                    modified_timestamp_in_ms,
                }),
                (false, true) => files.push(File {
                    path: stripped_path,
                    modified_timestamp_in_ms,
                    size: metadata.len(),
                }),
                (_, _) => {}
            }
        }
        Ok((directories, files))
    }

    #[allow(clippy::too_many_arguments)]
    fn save_file_chunk(
        &self,
        dir_path: &str,
        file_name: &str,
        file_size: u64,
        chunk_data: &Vec<u8>,
        chunk_id: u64,
        chunk_count: u64,
        chunk_offset: u64,
    ) -> Result<(), AnyError> {
        // get related dir and file path names
        let (full_path, target_path_name, temp_path_name) =
            get_upload_path_names(&self.config.root_path, dir_path, file_name)?;
        trace!("save_file_chunk(), full_path={:?}", &full_path);
        trace!(
            "save_file_chunk(), target_path_name={:?}",
            &target_path_name
        );
        trace!("save_file_chunk(), temp_path_name={:?}", &temp_path_name);

        // check if this is the first or last chunk, or both
        let first_chunk = chunk_id == 0;
        let last_chunk = chunk_id == (chunk_count - 1);
        trace!(
            "save_file_chunk(), first_chunk={}, last_chunk={}",
            first_chunk,
            last_chunk
        );

        // let mut succeeded = false;
        let write_result = match (first_chunk, last_chunk) {
            (true, true) => write_file(&target_path_name, chunk_data),
            (true, false) => {
                write_first_chunk(&target_path_name, &temp_path_name, chunk_data, file_size)
            }
            (false, false) => {
                write_chunk(&temp_path_name, chunk_data, chunk_offset, file_size, false)
            }
            (false, true) => write_last_chunk(
                &target_path_name,
                &temp_path_name,
                chunk_data,
                chunk_offset,
                file_size,
            ),
        };
        if write_result.is_err() {
            trace!("save_file_chunk(), write_result={:?}", write_result);
            let _ = remove_files(&target_path_name, &temp_path_name);
            write_result?;
        }
        Ok(())
    }

    fn discard_file_chunk(&self, dir_path: &str, file_name: &str) -> Result<(), AnyError> {
        let (full_path, target_path_name, temp_path_name) =
            get_upload_path_names(&self.config.root_path, dir_path, file_name)?;
        trace!("discard_file_chunk(), full_path={:?}", &full_path);
        trace!(
            "discard_file_chunk(), target_path_name={:?}",
            &target_path_name
        );
        trace!("discard_file_chunk(), temp_path_name={:?}", &temp_path_name);
        remove_files(&target_path_name, &temp_path_name)?;
        Ok(())
    }

    fn create_dir(&self, dir_path: &str, dir_name: &str) -> Result<(), AnyError> {
        let full_path = get_valid_joined_path(&self.config.root_path, dir_path)?;
        let _ = validate_name(dir_name)?;
        let target = full_path.join(dir_name);
        trace!("create_dir(), target={:?}", target);
        std::fs::create_dir(target)?;
        Ok(())
    }

    fn copy_file(&self, file_path_name: &str, dir_path: &str) -> Result<(), AnyError> {
        let from = get_valid_joined_path(&self.config.root_path, file_path_name)?;
        if from.is_file() {
            let file_name = from.file_name().ok_or_else(|| anyhow!("no file name"))?;
            let to = get_valid_joined_path(&self.config.root_path, dir_path)?.join(file_name);
            trace!("copy_file(), is_file, from={:?}, to={:?}", &from, &to);
            std::fs::copy(&from, &to)?;
        } else if from.is_dir() {
            use fs_extra::dir::{copy, CopyOptions};
            let to = get_valid_joined_path(&self.config.root_path, dir_path)?;
            trace!("copy_file(), is_dir, from={:?}, to={:?}", &from, &to);
            copy(from, to, &CopyOptions::new())?;
        }
        Ok(())
    }

    fn delete_file(&self, file_path_name: &str) -> Result<(), AnyError> {
        let full_name = get_valid_joined_path(&self.config.root_path, file_path_name)?;
        trace!("delete_file(), full_name={:?}", &full_name);
        if full_name.is_file() {
            std::fs::remove_file(full_name)?;
        } else if full_name.is_dir() {
            std::fs::remove_dir_all(full_name)?;
        }
        Ok(())
    }

    fn move_file(&self, file_path_name: &str, dir_path: &str) -> Result<(), AnyError> {
        let from = get_valid_joined_path(&self.config.root_path, file_path_name)?;
        let file_name = from.file_name().ok_or_else(|| anyhow!("no file name"))?;
        let to = get_valid_joined_path(&self.config.root_path, dir_path)?.join(file_name);
        trace!("move_file(), from={:?}, to={:?}", &from, &to);
        std::fs::rename(&from, &to)?;
        Ok(())
    }

    fn rename_file(&self, file_path_name: &str, new_name: &str) -> Result<(), AnyError> {
        let from = get_valid_joined_path(&self.config.root_path, file_path_name)?;
        let _ = validate_name(new_name)?;
        let mut to = from.clone();
        to.set_file_name(new_name);
        trace!("rename_file(), from={:?}, to={:?}", &from, &to);
        std::fs::rename(&from, &to)?;
        Ok(())
    }
}

#[tonic::async_trait]
impl ServaManager for ServaManagerServiceImpl {
    async fn list_dir(&self, request: TonicListDirReq) -> Result<TonicListDirResp, Status> {
        let dir_path = request.get_ref().dir_path.clone();
        debug!("list_dir(), dir_path={}", dir_path);
        let (directories, files) = self
            .get_dir_entries(&dir_path)
            .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        let reply = ListDirResponse {
            dir_path,
            directories,
            files,
        };
        Ok(Response::new(reply))
    }

    async fn get_config(&self, _req: TonicGetCfgReq) -> Result<TonicGetCfgResp, Status> {
        debug!("get_config()");
        // Discard request since no information inside
        let address = self
            .config
            .available_ip
            .iter()
            .map(|ip| Address {
                host: ip.to_string(),
                port: self.config.port as u32,
            })
            .collect();
        let reply = GetConfigResponse {
            root: self.config.root_relative.clone(),
            root_canonical: self.config.root_absolute.clone(),
            prefix: self.config.prefix.clone(),
            permission: Some(__self.config.permission.clone()),
            address,
        };
        Ok(Response::new(reply))
    }

    async fn upload_file_chunk(
        &self,
        request: TonicUploadFileChunkReq,
    ) -> Result<TonicUploadFileChunkResp, Status> {
        if !self.config.allow_upload {
            return Err(Status::new(Code::PermissionDenied, "Upload not allowed"));
        }
        let dir_path = request.get_ref().dir_path.as_str();
        let file_name = request.get_ref().file_name.as_str();
        let file_size = request.get_ref().file_size;
        let _file_hash = request.get_ref().file_hash.as_str(); // no use for now
        let abort = request.get_ref().abort;
        let chunk_data = &request.get_ref().chunk_data;
        let chunk_id = request.get_ref().chunk_id;
        let chunk_count = request.get_ref().chunk_count;
        let chunk_offset = request.get_ref().chunk_offset;
        let _chunk_size = request.get_ref().chunk_size; // no use for now
        let _chunk_hash = request.get_ref().chunk_hash.as_str(); // no use for now
        debug!(
            "upload_file_chunk(), dir={}, file={}, abort={}",
            dir_path, file_name, abort
        );
        let result = match abort {
            false => self.save_file_chunk(
                dir_path,
                file_name,
                file_size,
                chunk_data,
                chunk_id,
                chunk_count,
                chunk_offset,
            ),
            true => self.discard_file_chunk(dir_path, file_name),
        };
        match result {
            Err(e) => Err(Status::new(Code::Internal, e.to_string())),
            Ok(_) => Ok(Response::new(UploadFileChunkResponse {})),
        }
    }

    async fn manage_dir_or_file(
        &self,
        request: TonicManageDirOrFileReq,
    ) -> Result<TonicManageDirOrFileResp, Status> {
        if !self.config.allow_manage {
            return Err(Status::new(Code::PermissionDenied, "Manage not allowed"));
        }
        let file_path_name = request.get_ref().file_path_name.as_str();
        let dir_path = request.get_ref().dir_path.as_str();
        let target = request.get_ref().target.as_str();
        let operation = request.get_ref().operation();
        let operation_result = match operation {
            Operation::CreateDir => self.create_dir(dir_path, target),
            Operation::CopyFile => self.copy_file(file_path_name, dir_path),
            Operation::DeleteFile => self.delete_file(file_path_name),
            Operation::MoveFile => self.move_file(file_path_name, dir_path),
            Operation::RenameFile => self.rename_file(file_path_name, target),
        };
        let _ = operation_result.map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok(Response::new(ManageDirOrFileResponse {}))
    }
}

pub fn get_serva_manager(server_info: &ServerInfo) -> ServaManagerServerImpl {
    let config = Config::from(server_info);
    ServaManagerServer::new(ServaManagerServiceImpl { config })
}
