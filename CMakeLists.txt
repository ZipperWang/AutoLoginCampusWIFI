cmake_minimum_required(VERSION 3.29)
project(AutoLogin)

set(CMAKE_CXX_STANDARD 11)

# 设置 OpenSSL 头文件路径
set(OPENSSL_INCLUDE_DIR "${CMAKE_SOURCE_DIR}/include")
# 设置 OpenSSL 库文件路径
set(OPENSSL_LIBRARIES "${CMAKE_SOURCE_DIR}/lib")
include_directories(include)
link_directories(lib)
# 包含 OpenSSL 头文件目录
include_directories(${OPENSSL_INCLUDE_DIR})



add_executable(AutoLogin main.cpp
        include/encode.h
        include/HttpRequest.h
        include/Tools.h
        include/WIFI.h
        include/FileHandler.h)

# 链接 OpenSSL 库
# 设置链接 OpenSSL 的库文件
target_link_libraries(AutoLogin ${OPENSSL_LIBRARIES}/libssl.lib ${OPENSSL_LIBRARIES}/libcrypto.lib curl libcurl wininet)
