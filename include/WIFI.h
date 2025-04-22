//
// Created by ZipperWang on 2025/4/22.
//

#ifndef UNTITLED2_STRUCT_H
#include <iostream>
#include <fstream>
#include <cstdlib>
#include <string>
#include <chrono>

#define UNTITLED2_STRUCT_H


void connectOpenWiFi(const std::string& ssid) {
    // 生成临时 WiFi 配置 XML（无加密）
    std::string profileXml =
            "<?xml version=\"1.0\"?>"
            "<WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\">"
            "<name>" + ssid + "</name>"
                              "<SSIDConfig><SSID><name>" + ssid + "</name></SSID></SSIDConfig>"
                                                                  "<connectionType>ESS</connectionType>"
                                                                  "<connectionMode>auto</connectionMode>"
                                                                  "<MSM>"
                                                                  "<security>"
                                                                  "<authEncryption>"
                                                                  "<authentication>open</authentication>"
                                                                  "<encryption>none</encryption>"
                                                                  "<useOneX>false</useOneX>"
                                                                  "</authEncryption>"
                                                                  "</security>"
                                                                  "</MSM>"
                                                                  "</WLANProfile>";

    // 写入临时配置文件
    std::string profilePath = "wifi_open_profile.xml";
    std::ofstream out(profilePath);
    out << profileXml;
    out.close();

    // 添加配置文件
    std::string addCmd = "netsh wlan add profile filename=\"" + profilePath + "\"";
    system(addCmd.c_str());

    // 尝试连接
    std::string connectCmd = "netsh wlan connect name=\"" + ssid + "\"";
    system(connectCmd.c_str());

}

bool AddSelfToStartup(const std::wstring& appName) {
    // 获取当前程序的完整路径
    wchar_t path[MAX_PATH];
    GetModuleFileNameW(NULL, path, MAX_PATH);

    // 打开注册表项
    HKEY hKey;
    LONG result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            L"Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            0, KEY_WRITE, &hKey
    );

    if (result != ERROR_SUCCESS) {
        std::wcerr << L"无法打开注册表键！" << std::endl;
        return false;
    }

    // 写入自启动项
    result = RegSetValueExW(
            hKey,
            appName.c_str(), // 项名称
            0, REG_SZ,
            (const BYTE*)path,
            static_cast<DWORD>((wcslen(path) + 1) * sizeof(wchar_t))
    );

    RegCloseKey(hKey);

    if (result == ERROR_SUCCESS) {
        std::wcout << L"成功添加开机自启：" << path << std::endl;
        return true;
    } else {
        std::wcerr << L"设置注册表失败！" << std::endl;
        return false;
    }
}

std::string getMillisecondTimestampString() {
    auto now = std::chrono::system_clock::now();
    auto millis = std::chrono::duration_cast<std::chrono::milliseconds>(now.time_since_epoch()).count();
    return std::to_string(millis);
}

#endif //UNTITLED2_STRUCT_H
