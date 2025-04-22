//
// Created by ZipperWang on 2025/4/22.
//

#ifndef UNTITLED2_TOOLS_H

#include <iostream>
#include "json.hpp"

#define UNTITLED2_TOOLS_H

using namespace nlohmann;
std::string get_tocken(std::string response_data){
    // 解析 JSONP：应为 jsonp({...})
    if (response_data.find("jsonp(") == 0 && response_data.back() == ')') {
        std::string json_str = response_data.substr(6, response_data.size() - 7); // 去掉 jsonp() 包装
        try {
            auto json_obj = json::parse(json_str);
            if (json_obj.contains("challenge")) {
                std::string challenge = json_obj["challenge"];
                std::cout << "获取的token: " << challenge << std::endl;
                return challenge;
            } else {
                std::cerr << "未获取到 challenge 字段" << std::endl;
            }
        } catch (json::parse_error& e) {
            std::cerr << "JSON 解析失败: " << e.what() << std::endl;
        }
    } else {
        std::cerr << "返回数据格式错误，非 JSONP" << std::endl;
    }

    return "";
};

// 从 JSONP 文本中提取 challenge 值
std::string extract_challenge(const std::string& response) {
    std::string key = R"("challenge":")";
    size_t start = response.find(key);
    if (start == std::string::npos) {
        std::cerr << "未找到 challenge 字段" << std::endl;
        return "";
    }

    start += key.length();  // 跳过字段名和引号
    size_t end = response.find("\"", start);
    if (end == std::string::npos) {
        std::cerr << "challenge 字段未正确闭合" << std::endl;
        return "";
    }

    return response.substr(start, end - start);
}

// 提取指定字段的值
std::string extract_field(const std::string& response, const std::string& field_name) {
    std::string key = "\"" + field_name + "\":\"";
    size_t start = response.find(key);
    if (start == std::string::npos) {
        std::cerr << "未找到字段: " << field_name << std::endl;
        return "";
    }

    start += key.length();  // 跳过字段名
    size_t end = response.find("\"", start);
    if (end == std::string::npos) {
        std::cerr << "字段未正确闭合: " << field_name << std::endl;
        return "";
    }

    return response.substr(start, end - start);
}

std::string generate_info(const std::string& username, const std::string& password,
                          const std::string& ip, const std::string& ac_id, const std::string& enc_ver) {
    return "{\"username\":\"" + username + "\","
                                           "\"password\":\"" + password + "\","
                                                                          "\"ip\":\"" + ip + "\","
                                                                                             "\"acid\":\"" + ac_id + "\","
                                                                                                                     "\"enc_ver\":\"" + enc_ver + "\"}";
}

std::string generate_chksum(const std::string& token,
                            const std::string& username,
                            const std::string& md5,
                            const std::string& ac_id,
                            const std::string& ip,
                            const std::string& n,
                            const std::string& vtype,
                            const std::string& encrypted_info)
{
    return token + username +
           token + md5 +
           token + ac_id +
           token + ip +
           token + n +
           token + vtype +
           token + encrypted_info;
}

// 判断是否是 URL 中允许的字符
bool is_unreserved(char c) {
    return std::isalnum(static_cast<unsigned char>(c)) || c == '-' || c == '_' || c == '.' || c == '~';
}

// 简单的 URL 编码函数，仅用于参数值编码
std::string simple_url_encode(const std::string& value) {
    std::ostringstream encoded;
    for (unsigned char c : value) {
        if (is_unreserved(c)) {
            encoded << c;
        } else {
            encoded << '%' << std::uppercase << std::setw(2) << std::setfill('0') << std::hex << int(c);
        }
    }
    return encoded.str();
}

// 拆分并编码参数值，重新拼接 URL
std::string encode_url_params(const std::string& raw_url) {
    size_t q_pos = raw_url.find('?');
    if (q_pos == std::string::npos) return raw_url; // 无参数，原样返回

    std::string base = raw_url.substr(0, q_pos);
    std::string query = raw_url.substr(q_pos + 1);

    std::ostringstream encoded_url;
    encoded_url << base << '?';

    std::istringstream iss(query);
    std::string pair;
    bool first = true;
    while (std::getline(iss, pair, '&')) {
        if (!first) encoded_url << "&";
        first = false;

        size_t eq = pair.find('=');
        if (eq != std::string::npos) {
            std::string key = pair.substr(0, eq);
            std::string value = pair.substr(eq + 1);
            encoded_url << key << "=" << simple_url_encode(value);
        } else {
            // 没有等号的参数也保留
            encoded_url << simple_url_encode(pair);
        }
    }

    return encoded_url.str();
}


#endif //UNTITLED2_TOOLS_H
