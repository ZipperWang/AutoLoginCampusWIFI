//
// Created by ZipperWang on 2025/4/21.
//
#include <iostream>
#include <iomanip>
#include <openssl/hmac.h>
#include <openssl/md5.h>
#include <openssl/sha.h>
#include <cstring>
#include <sstream>
#include <vector>
#include <string>
#include <cmath>

#ifndef UNTITLED2_ENCODE_H
#define UNTITLED2_ENCODE_H

const char PADCHAR = '=';
const std::string ALPHA = "LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA";

// Function to get byte value from the character
int _getbyte(const std::string& s, size_t i) {
    unsigned char x = s[i];
    if (x > 255) {
        std::cerr << "INVALID_CHARACTER_ERR: DOM Exception 5" << std::endl;
        exit(0);
    }
    return x;
}

// Function to perform custom base64 encoding
std::string get_base64(const std::string& s) {
    size_t i = 0;
    unsigned int b10 = 0;
    std::string result;
    size_t imax = s.size() - s.size() % 3;

    if (s.empty()) return s;

    for (i = 0; i < imax; i += 3) {
        b10 = (_getbyte(s, i) << 16) | (_getbyte(s, i + 1) << 8) | _getbyte(s, i + 2);
        result.push_back(ALPHA[(b10 >> 18)]);
        result.push_back(ALPHA[((b10 >> 12) & 63)]);
        result.push_back(ALPHA[((b10 >> 6) & 63)]);
        result.push_back(ALPHA[(b10 & 63)]);
    }

    i = imax;
    if (s.size() - imax == 1) {
        b10 = _getbyte(s, i) << 16;
        result.append(1, ALPHA[(b10 >> 18)]);
        result.append(1, ALPHA[((b10 >> 12) & 63)]);
        result.append(1, PADCHAR);
        result.append(1, PADCHAR);
    }
    else if (s.size() - imax == 2) {
        b10 = (_getbyte(s, i) << 16) | (_getbyte(s, i + 1) << 8);
        result.append(1, ALPHA[(b10 >> 18)]);
        result.append(1, ALPHA[((b10 >> 12) & 63)]);
        result.append(1, ALPHA[((b10 >> 6) & 63)]);
        result.append(1, PADCHAR);
    }

    return result;
}
// 获取 HMAC-MD5 的十六进制表示
std::string get_hmac_md5(const std::string& password, const std::string& token) {
    unsigned char* result;
    unsigned int len = MD5_DIGEST_LENGTH;

    // 使用 HMAC-MD5 算法
    result = HMAC(EVP_md5(), token.c_str(), token.length(), (unsigned char*)password.c_str(), password.length(), nullptr, &len);

    // 转换为十六进制字符串
    std::ostringstream ss;
    for (unsigned int i = 0; i < len; ++i) {
        ss << std::hex << std::setw(2) << std::setfill('0') << (int)result[i];
    }
    return ss.str();
}

std::string get_sha1(const std::string& value) {
    unsigned char hash[SHA_DIGEST_LENGTH];

    // Compute the SHA-1 hash
    SHA1(reinterpret_cast<const unsigned char*>(value.c_str()), value.length(), hash);

    // Convert the hash to a hexadecimal string
    std::ostringstream ss;
    for (int i = 0; i < SHA_DIGEST_LENGTH; ++i) {
        ss << std::hex << std::setw(2) << std::setfill('0') << static_cast<int>(hash[i]);
    }
    return ss.str();
}

std::vector<unsigned char> force(const std::string& msg) {
    std::vector<unsigned char> ret;
    for (char c : msg) {
        ret.push_back(static_cast<unsigned char>(c));
    }
    return ret;
}

int ordat(const std::string& msg, size_t idx) {
    if (msg.size() > idx) {
        return static_cast<int>(msg[idx]);
    }
    return 0;
}

std::vector<unsigned int> sencode(const std::string& msg, bool key) {
    size_t l = msg.size();
    std::vector<unsigned int> pwd;

    for (size_t i = 0; i < l; i += 4) {
        unsigned int value = ordat(msg, i) |
                             ordat(msg, i + 1) << 8 |
                             ordat(msg, i + 2) << 16 |
                             ordat(msg, i + 3) << 24;
        pwd.push_back(value);
    }

    if (key) {
        pwd.push_back(l);
    }

    return pwd;
}

std::string lencode(std::vector<unsigned int>& msg, bool key) {
    size_t l = msg.size();
    size_t ll = (l - 1) << 2;
    if (key) {
        int m = msg[l - 1];
        if (m < ll - 3 || m > ll) {
            return "";
        }
        ll = m;
    }

    std::ostringstream result;
    for (size_t i = 0; i < l; ++i) {
        result << static_cast<char>(msg[i] & 0xff)
               << static_cast<char>((msg[i] >> 8) & 0xff)
               << static_cast<char>((msg[i] >> 16) & 0xff)
               << static_cast<char>((msg[i] >> 24) & 0xff);
    }

    if (key) {
        return result.str().substr(0, ll);
    }

    return result.str();
}

std::string get_xencode(std::string& msg, std::string& key) {
    if (msg.empty()) {
        return "";
    }

    std::vector<unsigned int> pwd = sencode(msg, true);
    std::vector<unsigned int> pwdk = sencode(key, false);

    if (pwdk.size() < 4) {
        pwdk.resize(4, 0);
    }

    size_t n = pwd.size() - 1;
    unsigned int z = pwd[n];
    unsigned int y = pwd[0];
    unsigned int c = 0x86014019 | 0x183639A0;
    unsigned int m = 0, e = 0, p = 0;
    size_t q = std::floor(6 + 52 / (n + 1));
    unsigned int d = 0;

    while (q > 0) {
        d = (d + c) & (0x8CE0D9BF | 0x731F2640);
        e = d >> 2 & 3;
        p = 0;
        while (p < n) {
            y = pwd[p + 1];
            m = z >> 5 ^ y << 2;
            m = m + ((y >> 3 ^ z << 4) ^ (d ^ y));
            m = m + (pwdk[(p & 3) ^ e] ^ z);
            pwd[p] = (pwd[p] + m) & (0xEFB8D130 | 0x10472ECF);
            z = pwd[p];
            p++;
        }

        y = pwd[0];
        m = z >> 5 ^ y << 2;
        m = m + ((y >> 3 ^ z << 4) ^ (d ^ y));
        m = m + (pwdk[(p & 3) ^ e] ^ z);
        pwd[n] = (pwd[n] + m) & (0xBB390742 | 0x44C6F8BD);
        z = pwd[n];
        q--;
    }

    return lencode(pwd, false);
}

#endif //UNTITLED2_ENCODE_H
