use std::{io, process::Command};

pub fn campus_wifi(connect: bool) -> io::Result<Option<u32>> {
    let list = Command::new("nmcli")
        .env("LC_ALL", "C")
        .args([
            "--wait",
            "3",
            "--terse",
            "--fields",
            "IN-USE,SSID,DEVICE",
            "device",
            "wifi",
            "list",
            "--rescan",
            "no",
        ])
        .output()?;
    let output = String::from_utf8_lossy(&list.stdout);
    let device = output
        .lines()
        .find_map(|line| line.strip_prefix("*:NWAFU:"));
    if connect && device.is_none() {
        let result = Command::new("nmcli")
            .env("LC_ALL", "C")
            .args(["--wait", "15", "device", "wifi", "connect", "NWAFU"])
            .output()?;
        if !result.status.success() {
            return Err(io::Error::other("NetworkManager未能连接NWAFU"));
        }
    } else if !list.status.success() {
        return Err(io::Error::other("NetworkManager不可用"));
    }
    Ok(device.and_then(|name| {
        if_addrs::get_if_addrs()
            .ok()?
            .into_iter()
            .find(|interface| interface.name == name)?
            .index
    }))
}
