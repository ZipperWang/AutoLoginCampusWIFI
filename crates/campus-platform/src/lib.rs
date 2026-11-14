use std::{
    fs::{self, File, OpenOptions},
    io,
    net::IpAddr,
};

pub use if_watch::tokio::IfWatcher;
pub use keyring::{Entry, Error as CredentialError};

#[cfg_attr(windows, path = "windows.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "macos", path = "macos.rs")]
mod wifi;
pub use wifi::campus_wifi;

pub fn interface_for(ip: IpAddr) -> Option<if_addrs::Interface> {
    if_addrs::get_if_addrs()
        .ok()?
        .into_iter()
        .find(|interface| interface.ip() == ip)
}

pub fn credential(username: &str) -> Result<Entry, CredentialError> {
    Entry::new("tech.nwafu.campus-cli", username)
}

pub fn lock_login() -> io::Result<File> {
    let dir = dirs::data_local_dir()
        .ok_or_else(|| io::Error::other("找不到用户数据目录"))?
        .join("NWAFU-Campus");
    fs::create_dir_all(&dir)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(dir.join("login.lock"))?;
    match lock.try_lock() {
        Ok(()) => Ok(lock),
        Err(std::fs::TryLockError::WouldBlock) => Err(io::ErrorKind::WouldBlock.into()),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}
