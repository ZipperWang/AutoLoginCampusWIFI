use std::{io, ptr, slice};
use windows_sys::Win32::NetworkManagement::{
    IpHelper::{ConvertInterfaceGuidToLuid, ConvertInterfaceLuidToIndex},
    Ndis::NET_LUID_LH,
    WiFi::*,
};

pub fn campus_wifi(connect: bool) -> io::Result<Option<u32>> {
    let mut handle = ptr::null_mut();
    let mut version = 0;
    let mut list = ptr::null_mut();
    let mut found = None;
    //系统分配的接口列表和连接属性在句柄关闭前释放
    unsafe {
        let code = WlanOpenHandle(2, ptr::null(), &mut version, &mut handle);
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let mut code = WlanEnumInterfaces(handle, ptr::null(), &mut list);
        if code == 0 {
            let interfaces = slice::from_raw_parts(
                (*list).InterfaceInfo.as_ptr(),
                (*list).dwNumberOfItems as usize,
            );
            for interface in interfaces {
                if interface.isState != wlan_interface_state_connected {
                    continue;
                }
                let mut size = 0;
                let mut data = ptr::null_mut();
                let result = WlanQueryInterface(
                    handle,
                    &interface.InterfaceGuid,
                    wlan_intf_opcode_current_connection,
                    ptr::null(),
                    &mut size,
                    &mut data,
                    ptr::null_mut(),
                );
                if result == 0 {
                    if size as usize >= size_of::<WLAN_CONNECTION_ATTRIBUTES>() {
                        let ssid = &(*data.cast::<WLAN_CONNECTION_ATTRIBUTES>())
                            .wlanAssociationAttributes
                            .dot11Ssid;
                        if ssid.ucSSID.get(..ssid.uSSIDLength as usize) == Some(b"NWAFU") {
                            let mut luid = NET_LUID_LH::default();
                            let mut index = 0;
                            if ConvertInterfaceGuidToLuid(&interface.InterfaceGuid, &mut luid) == 0
                                && ConvertInterfaceLuidToIndex(&luid, &mut index) == 0
                            {
                                found = Some(index);
                            }
                        }
                    }
                    WlanFreeMemory(data);
                    if found.is_some() {
                        break;
                    }
                } else if !connect {
                    code = result;
                }
            }
            if connect && found.is_none() {
                code = 1168;
                let mut ssid = DOT11_SSID {
                    uSSIDLength: 5,
                    ..Default::default()
                };
                ssid.ucSSID[..5].copy_from_slice(b"NWAFU");
                let params = WLAN_CONNECTION_PARAMETERS {
                    wlanConnectionMode: wlan_connection_mode_discovery_unsecure,
                    pDot11Ssid: &mut ssid,
                    dot11BssType: dot11_BSS_type_infrastructure,
                    ..Default::default()
                };
                for interface in interfaces {
                    code = WlanConnect(handle, &interface.InterfaceGuid, &params, ptr::null());
                    if code == 0 {
                        break;
                    }
                }
            }
            WlanFreeMemory(list.cast());
        }
        WlanCloseHandle(handle, ptr::null());
        if code != 0 && found.is_none() {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
    }
    Ok(found)
}
