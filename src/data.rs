use anyhow::anyhow;
use std::{
    fmt::{Display, Formatter},
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
};

type AnyError = anyhow::Error;

const DEFAULT_PREFIXES: [&str; 3] = ["shared-files", "_shared_files_", "__shared_files__"];

#[derive(rust_embed::RustEmbed)]
#[folder = "webapp/build"]
pub struct Asset;

fn generate_prefix() -> Result<&'static str, AnyError> {
    for prefix in DEFAULT_PREFIXES {
        if Asset::get(prefix).is_none() {
            // return the 1st prefix which is NOT in asset
            return Ok(prefix);
        }
    }
    Err(anyhow!("Cannot found proper prefix"))
}

fn get_available_ip(ip: IpAddr) -> Result<Vec<IpAddr>, AnyError> {
    if ip.is_loopback() {
        return Ok(vec![ip]);
    }
    if !ip.is_unspecified() {
        return Ok(vec![ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))]);
    }
    Ok(get_if_addrs::get_if_addrs()?
        .iter()
        .map(|interface| interface.ip())
        .collect())
}

#[derive(Debug)]
pub struct ServerInfo {
    pub arg_path: String,
    pub arg_allow_cors: bool,
    pub arg_allow_manage: bool,
    pub arg_allow_upload: bool,
    pub arg_allow_download: bool,
    pub arg_ip: IpAddr,
    pub arg_port: u16,
    pub root: PathBuf,
    pub root_canonical: String,
    pub prefix: String,
    pub available_ip: Vec<IpAddr>,
}

impl ServerInfo {
    pub fn new(
        path: &str,
        allow_cors: bool,
        allow_manage: bool,
        allow_upload: bool,
        allow_download: bool,
        ip: IpAddr,
        port: u16,
    ) -> Result<Self, AnyError> {
        let root = PathBuf::from(path).canonicalize()?;
        let root_canonical = root
            .to_str()
            .ok_or_else(|| anyhow!("canonical"))?
            .to_string();
        let prefix = format!("/{}/", generate_prefix()?);
        let available_ip = get_available_ip(ip)?;
        Ok(ServerInfo {
            arg_path: path.to_string(),
            arg_allow_cors: allow_cors,
            arg_allow_manage: allow_manage,
            arg_allow_upload: allow_upload,
            arg_allow_download: allow_download,
            arg_ip: ip,
            arg_port: port,
            root,
            root_canonical,
            prefix,
            available_ip,
        })
    }
}

impl Display for ServerInfo {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        writeln!(
            f,
            "    ip:{}; port:{}; path:{}",
            self.arg_ip, self.arg_port, self.arg_path
        )?;
        writeln!(
            f,
            "    allow_cors:{}; allow_manage:{}; allow_upload:{}; allow_download:{}",
            self.arg_allow_cors,
            self.arg_allow_manage,
            self.arg_allow_upload,
            self.arg_allow_download
        )?;
        write!(
            f,
            "    root:{}; prefix:{}",
            self.root_canonical, self.prefix
        )?;
        Ok(())
    }
}

pub fn get_valid_joined_path<P>(base: &Path, suffix: P) -> Result<PathBuf, AnyError>
where
    P: AsRef<Path>,
{
    let joined_path = base.join(suffix).canonicalize()?;
    if !joined_path.starts_with(base) {
        return Err(anyhow!("accessing parent directory is forbidden"));
    }
    Ok(joined_path)
}
