use clap::Parser;
use data::ServerInfo;
use multiplex::MultiplexService;
use std::net::{IpAddr, SocketAddr};
use tower::make::Shared;

mod data;
mod grpc;
mod multiplex;
mod serve;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value = ".")]
    dir: String,
    #[clap(short, long, value_parser, default_value = "0.0.0.0")]
    ip: IpAddr,
    #[clap(short, long, value_parser, default_value_t = 3000)]
    port: u16,
    #[clap(long, value_parser)]
    enable_cors: bool,
    #[clap(long, value_parser)]
    enable_manage: bool,
    #[clap(long, value_parser)]
    disable_upload: bool,
    #[clap(long, value_parser)]
    disable_download: bool,
}

#[tokio::main]
/*async*/
async fn main() {
    // init logger
    env_logger::builder().format_timestamp_millis().init();

    // parse args
    let args = Args::parse();

    // generate server info from args
    let server_info = ServerInfo::new(
        &args.dir,
        args.enable_cors,
        args.enable_manage,
        !args.disable_upload,
        !args.disable_download,
        args.ip,
        args.port,
    )
    .unwrap();
    println!("Server Info:\n{}", server_info);

    // generate service from server_info
    let file_serve_service = serve::get_serve_file_service(&server_info);
    let grpc_service = tonic_web::enable(grpc::get_serva_manager(&server_info));
    let multiplex_service = MultiplexService::new(file_serve_service, grpc_service);
    let make_service = Shared::new(multiplex_service);

    // run it
    let addr = SocketAddr::from((server_info.arg_ip, server_info.arg_port));
    for ip in server_info.available_ip {
        println!("listening on {}:{}", ip, server_info.arg_port);
    }
    axum::Server::bind(&addr).serve(make_service).await.unwrap();
}
