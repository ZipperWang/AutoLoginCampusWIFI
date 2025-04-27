//
// Created by ZipperWang on 2025/4/22.
//

#ifndef UNTITLED2_STRUCT_H
#include <iostream>
#include <fstream>
#include <cstdlib>
#include <string>
#include <chrono>
#include <windows.h>
#include <wininet.h>
#include <cstdlib>
#define UNTITLED2_STRUCT_H

#undef UNICODE
#undef _UNICODE


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

bool IsConnectedToInternet() {
    HINTERNET hInternet = InternetOpen("NetDetect", INTERNET_OPEN_TYPE_PRECONFIG, NULL, NULL, 0);
    if (!hInternet) return false;

    HINTERNET hUrl = InternetOpenUrl(hInternet, "http://www.gstatic.com/generate_204", NULL, 0, INTERNET_FLAG_NO_UI, 0);
    if (hUrl) {
        InternetCloseHandle(hUrl);
        InternetCloseHandle(hInternet);
        return true;
    }

    InternetCloseHandle(hInternet);
    return false;
}

bool isInternetAccessible()
{
    HINTERNET hInternet = InternetOpen(reinterpret_cast<LPCSTR>(L"MyInternetChecker"), INTERNET_OPEN_TYPE_PRECONFIG, NULL, NULL, 0);
    if (hInternet == NULL)
        return false;

    HINTERNET hConnect = InternetOpenUrl(
            hInternet,
            reinterpret_cast<LPCSTR>(L"http://www.msftconnecttest.com/connecttest.txt"),
            NULL,
            0,
            INTERNET_FLAG_NO_CACHE_WRITE,
            0);

    bool result = false;
    if (hConnect != NULL)
    {
        result = true; // 能打开网址
        InternetCloseHandle(hConnect);
    }

    InternetCloseHandle(hInternet);
    return result;
}

bool isInternetAvailable() {
    int exitCode = system("ping -n 1 www.baidu.com > nul"); // Windows
    return exitCode == 0;
}


bool registerSilentStartupViaVBS(const std::wstring& appName) {
    // 获取当前程序路径
    wchar_t exePath[MAX_PATH];
    GetModuleFileNameW(NULL, exePath, MAX_PATH);
    std::wstring exeFullPath = exePath;

    // 获取当前目录（vbs 文件生成在这里）
    size_t lastSlash = exeFullPath.find_last_of(L"\\/");
    std::wstring exeDir = exeFullPath.substr(0, lastSlash);
    std::wstring vbsPath = exeDir + L"\\run_self_silent.vbs";

    // 创建 .vbs 文件（用于静默启动 .exe）
    std::wofstream vbsFile(vbsPath.c_str());
    if (!vbsFile.is_open()) {
        std::wcerr << L"VBS 文件创建失败！" << std::endl;
        return false;
    }

    vbsFile << L"Set WshShell = CreateObject(\"WScript.Shell\")\n";
    vbsFile << L"WshShell.Run \"" << exeFullPath << L"\", 0, False\n";
    vbsFile.close();

    // 写入注册表 -> HKEY_CURRENT_USER\...\Run
    HKEY hKey;
    if (RegOpenKeyExW(HKEY_CURRENT_USER,
                      L"Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                      0, KEY_SET_VALUE, &hKey) != ERROR_SUCCESS) {
        std::wcerr << L"打开注册表失败！" << std::endl;
        return false;
    }

    LONG result = RegSetValueExW(
            hKey,
            appName.c_str(),
            0,
            REG_SZ,
            (const BYTE*)vbsPath.c_str(),
            static_cast<DWORD>((vbsPath.size() + 1) * sizeof(wchar_t))
    );

    RegCloseKey(hKey);

    if (result != ERROR_SUCCESS) {
        std::wcerr << L"注册表写入失败！" << std::endl;
        return false;
    }

    std::wcout << L"已成功创建 VBS 并注册为开机自启：" << vbsPath << std::endl;
    return true;
}
// 自动生成网络检测 VBS 脚本，并注册开机自启
bool SetupNetCheckAutoStart(const std::wstring& regName) {
    // 获取当前 EXE 路径
    wchar_t exePath[MAX_PATH];
    GetModuleFileNameW(NULL, exePath, MAX_PATH);
    std::wstring exeFullPath = exePath;

    // 构造 VBS 脚本路径（与程序同目录）
    size_t pos = exeFullPath.find_last_of(L"\\/");
    std::wstring exeDir = exeFullPath.substr(0, pos);
    std::wstring vbsPath = exeDir + L"\\auto_check_net.vbs";

    // 写 VBS 内容
    std::wofstream vbsFile(vbsPath.c_str());
    if (!vbsFile.is_open()) return false;

    vbsFile << L"Set objShell = CreateObject(\"WScript.Shell\")\n";
    vbsFile << L"exePath = \"" << exeFullPath << L"\"\n";
    vbsFile << L"Do\n";
    vbsFile << L"    On Error Resume Next\n";
    vbsFile << L"    Set objXML = CreateObject(\"Microsoft.XMLHTTP\")\n";
    vbsFile << L"    objXML.Open \"GET\", \"http://www.msftconnecttest.com/connecttest.txt\", False\n";
    vbsFile << L"    objXML.Send\n";
    vbsFile << L"    If Err.Number <> 0 Or objXML.Status <> 200 Then\n";
    vbsFile << L"        objShell.Run \"\"\"\" & exePath & \"\"\"\", 0, False\n";
    vbsFile << L"    End If\n";
    vbsFile << L"    WScript.Sleep 60000\n";
    vbsFile << L"Loop\n";

    vbsFile.close();

    // 注册到开机启动（写注册表）
    HKEY hKey;
    if (RegOpenKeyExW(HKEY_CURRENT_USER,
                      L"Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                      0, KEY_SET_VALUE, &hKey) != ERROR_SUCCESS) {
        return false;
    }

    LONG res = RegSetValueExW(hKey, regName.c_str(), 0, REG_SZ,
                              (const BYTE*)vbsPath.c_str(),
                              static_cast<DWORD>((vbsPath.size() + 1) * sizeof(wchar_t)));

    RegCloseKey(hKey);
    return res == ERROR_SUCCESS;
}
std::string getMillisecondTimestampString() {
    auto now = std::chrono::system_clock::now();
    auto millis = std::chrono::duration_cast<std::chrono::milliseconds>(now.time_since_epoch()).count();
    return std::to_string(millis);
}

#endif //UNTITLED2_STRUCT_H
