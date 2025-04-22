//
// Created by ZipperWang on 2025/4/22.
//

#ifndef AUTOLOGIN_FILEHANDLER_H
#include <iostream>
#include <fstream>
#include <string>
#include <vector>
#define AUTOLOGIN_FILEHANDLER_H


class FileHandler {
public:
    // 构造函数
    FileHandler(const std::string& filename) : filename_(filename) {}

    // 检查文件是否存在
    bool fileExists() const {
        std::ifstream file(filename_);
        return file.good();
    }

    // 打开文件进行写入
    bool openForWrite() {
        outFile_.open(filename_, std::ios::out | std::ios::app);
        if (!outFile_) {
            std::cerr << "文件打开失败!" << std::endl;
            return false;
        }
        return true;
    }

    // 打开文件进行读取
    bool openForRead() {
        inFile_.open(filename_, std::ios::in);
        if (!inFile_) {
            std::cerr << "文件打开失败!" << std::endl;
            return false;
        }
        return true;
    }

    // 关闭文件
    void close() {
        if (outFile_.is_open()) {
            outFile_.close();
        }
        if (inFile_.is_open()) {
            inFile_.close();
        }
    }

    // 写入字符串到文件
    bool write(const std::string& content) {
        if (!outFile_.is_open() && !openForWrite()) {
            return false;
        }
        outFile_ << content << std::endl;
        return true;
    }

    // 读取文件内容到字符串
    bool read(std::string& content) {
        if (!inFile_.is_open() && !openForRead()) {
            return false;
        }
        std::getline(inFile_, content);
        return true;
    }

    // 读取所有文件内容并返回
    bool readAll(std::vector<std::string>& lines) {
        if (!inFile_.is_open() && !openForRead()) {
            return false;
        }
        std::string line;
        while (std::getline(inFile_, line)) {
            lines.push_back(line);
        }
        return true;
    }

    // 删除文件
    bool deleteFile() {
        if (remove(filename_.c_str()) != 0) {
            std::cerr << "文件删除失败!" << std::endl;
            return false;
        }
        return true;
    }

    //清空文件
    bool clearFile() {
        std::ofstream file(filename_.c_str(), std::ios::out | std::ios::trunc);
        if (!file.is_open()) {
            return false; // 文件无法打开
        }
        file.close();
        return true;
    }

private:
    std::string filename_;
    std::ofstream outFile_;
    std::ifstream inFile_;
};
#endif //AUTOLOGIN_FILEHANDLER_H
