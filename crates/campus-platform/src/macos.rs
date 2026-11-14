use objc2_core_wlan::CWWiFiClient;
use objc2_foundation::NSString;
use std::io;

pub fn campus_wifi(connect: bool) -> io::Result<Option<u32>> {
    unsafe {
        let client = CWWiFiClient::sharedWiFiClient();
        let interface = client
            .interface()
            .ok_or_else(|| io::Error::other("找不到无线网卡"))?;
        let found = interface
            .ssid()
            .is_some_and(|ssid| ssid.to_string() == "NWAFU");
        if connect && !found {
            let name = NSString::from_str("NWAFU");
            let networks = interface
                .scanForNetworksWithName_error(Some(&name))
                .map_err(|_| io::Error::other("无法扫描NWAFU，请检查无线网络权限"))?;
            let network = networks
                .anyObject()
                .ok_or_else(|| io::Error::other("附近没有NWAFU"))?;
            interface
                .associateToNetwork_password_error(&network, None)
                .map_err(|_| io::Error::other("系统未能连接NWAFU"))?;
        }
        let name = interface.interfaceName().map(|s| s.to_string());
        Ok(if found {
            if_addrs::get_if_addrs()?
                .into_iter()
                .find(|i| Some(&i.name) == name.as_ref())
                .and_then(|i| i.index)
        } else {
            None
        })
    }
}
