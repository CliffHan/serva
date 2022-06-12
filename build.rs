use const_format::formatcp;
use std::{
    fs::{create_dir, remove_dir_all},
    process::Command,
};

fn main() {
    const PROTO_DIR: &str = "./proto";
    const PROTO_FILE: &str = "api.proto";
    const PROTO_FILE_FULLNAME: &str = formatcp!("{}/{}", PROTO_DIR, PROTO_FILE);
    const OUTPUT_DIR: &str = "./src/generated";

    const WEBAPP_DIR: &str = "./webapp";
    const JS_GEN_DIR: &str = "./webapp/src/generated";
    const ARG_PROTO_PATH: &str = formatcp!("-I={}", PROTO_DIR);
    const ARG_JS_OUT: &str = formatcp!("--js_out=import_style=commonjs,binary:{}", JS_GEN_DIR);
    const ARG_GRPC_WEB_JS_OUT: &str = formatcp!(
        "--grpc-web_out=import_style=typescript,mode=grpcweb:{}",
        JS_GEN_DIR
    );

    // build proto server rs only
    println!("cargo:rerun-if-changed=proto");
    let _ = remove_dir_all(OUTPUT_DIR);
    create_dir(OUTPUT_DIR).expect("Failed to create dir for grpc rs");
    tonic_build::configure()
        .build_client(false)
        .out_dir(OUTPUT_DIR)
        .compile(&[PROTO_FILE_FULLNAME], &[PROTO_DIR])
        .expect("Failed to generate rust code from proto");

    // skip build of webapp when specified environment variable SKIP_WEBAPP
    if let Some(_) = option_env!("SKIP_BUILD_WEBAPP") {
        println!("cargo:warning={}", "skipped build process of webapp");
        return;
    }
    
    // build generate js
    println!("cargo:rerun-if-changed=proto");
    let _ = remove_dir_all(JS_GEN_DIR);
    create_dir(JS_GEN_DIR).expect("Failed to create dir for grpc js/ts");
    Command::new("protoc")
        .args([ARG_PROTO_PATH, PROTO_FILE, ARG_JS_OUT, ARG_GRPC_WEB_JS_OUT])
        .output()
        .expect("protoc failed to generate grpc js");

    // yarn install
    println!("cargo:rerun-if-changed=webapp/package.json");
    Command::new("yarn")
        .args(["install"])
        .current_dir(WEBAPP_DIR)
        .output()
        .expect("yarn install failed");

    // yarn build
    println!("cargo:rerun-if-changed=webapp/src,webapp/public");
    Command::new("yarn")
        .args(["build"])
        .current_dir(WEBAPP_DIR)
        .output()
        .expect("yarn build failed");

    println!("cargo:rerun-if-changed=src,webapp/build");
}
