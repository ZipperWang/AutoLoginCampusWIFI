//
// Created by ZipperWang on 2025/4/21.
//

#ifndef UNTITLED2_HTTPREQUEST_H
#define UNTITLED2_HTTPREQUEST_H


#include <iostream>
#include <string>
#include <curl/curl.h>

class HttpRequest {
public:
    HttpRequest() {
        curl_global_init(CURL_GLOBAL_DEFAULT);
        curl = curl_easy_init();

        headers = curl_slist_append(headers, "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36");
        headers = curl_slist_append(headers, "Referer: https://portal.nwafu.edu.cn/");
    }

    ~HttpRequest() {
        if (curl) {
            curl_easy_cleanup(curl);
        }
        curl_global_cleanup();
    }

    // 发送 GET 请求
    std::string get(std::basic_string<char> url) {
        std::string response;
        if (curl) {
            curl_easy_setopt(curl, CURLOPT_URL, url.c_str());
            curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, WriteCallback);
            curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response);
            // 设置响应头回调
            curl_easy_setopt(curl, CURLOPT_HEADERFUNCTION, HeaderCallback);
            curl_easy_setopt(curl, CURLOPT_HEADERDATA, nullptr);
            curl_easy_setopt(curl, CURLOPT_SSL_VERIFYPEER, 0L); // 不验证证书
            curl_easy_setopt(curl, CURLOPT_SSL_VERIFYHOST, 0L); // 不验证主机名

            curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);

            CURLcode res = curl_easy_perform(curl);
            if (res != CURLE_OK) {
                std::cerr << "GET request failed: " << curl_easy_strerror(res) << std::endl;
                return "";
            }
        }
        return response;
    }

    // 发送 POST 请求
    std::string post(std::string& url, std::string& data) {
        std::string response;

        if (curl) {
            curl_easy_setopt(curl, CURLOPT_URL, url.c_str());
            curl_easy_setopt(curl, CURLOPT_POSTFIELDS, data.c_str());
            curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, WriteCallback);
            curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response);
            // 设置响应头回调
            curl_easy_setopt(curl, CURLOPT_HEADERFUNCTION, HeaderCallback);
            curl_easy_setopt(curl, CURLOPT_HEADERDATA, nullptr);
            curl_easy_setopt(curl, CURLOPT_SSL_VERIFYPEER, 0L); // 不验证证书
            curl_easy_setopt(curl, CURLOPT_SSL_VERIFYHOST, 0L); // 不验证主机名

            curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);

            CURLcode res = curl_easy_perform(curl);
            if (res != CURLE_OK) {
                std::cerr << "POST request failed: " << curl_easy_strerror(res) << std::endl;
                return "";
            }
        }
        return response;
    }

private:
    CURL* curl;
    struct curl_slist* headers = nullptr;

    // 写回调函数，用于处理返回的数据
    static size_t WriteCallback(void* contents, size_t size, size_t nmemb, void* userp) {
        ((std::string*)userp)->append((char*)contents, size * nmemb);
        return size * nmemb;
    }

    // 打印响应头
    static size_t HeaderCallback(char* buffer, size_t size, size_t nitems, void* userdata) {
        size_t totalSize = size * nitems;
        std::string header_line(buffer, totalSize);
        return totalSize;
    }

    std::string url_encode(const std::string& str) {
        char* encoded = curl_easy_escape(curl, str.c_str(), str.length());
        std::string result(encoded);
        curl_free(encoded);
        return result;
    }
};




#endif //UNTITLED2_HTTPREQUEST_H
