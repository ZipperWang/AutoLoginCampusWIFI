#include <iostream>
#include "encode.h"
#include "HttpRequest.h"
#include "Tools.h"

using namespace std;
string username = "username";
string password = "password";
int main() {
    HttpRequest httpRequest;

    string full_url = "https://portal.nwafu.edu.cn/cgi-bin/get_challenge?username=2024011843&ip=&callback=jsonp";
    string getResponse = httpRequest.get(full_url);
    cout << "GET Response: " << getResponse << endl;
    cout << "GET Response: " << extract_field(getResponse, "client_ip") << endl;
    string tocken = extract_field(getResponse, "challenge");

    string ip = extract_field(getResponse, "client_ip");


    string hmd5 = get_hmac_md5(password, tocken);
    cout<< "hmd5: "<< hmd5 << endl;

    string info_str = generate_info(username, password, ip, "1", "srun_bx1");
    string encrypted_info = "{SRBX1}" + get_base64(get_xencode(info_str, tocken));
    cout<< "info: "<< encrypted_info << endl;

    string chksum_str = generate_chksum(tocken, username,  hmd5, "1", ip, "200", "1", encrypted_info);
    string chksum = get_sha1(chksum_str);
    cout<< "chksum: "<< chksum << endl;

    std::string query = "https://portal.nwafu.edu.cn/cgi-bin/srun_portal";
    query += "?callback=jsonp1583251661368";
    query += "&action=login";
    query += "&username=" + username;
    query += "&password={MD5}" + hmd5;
    query += "&os=Windows+10&name=Windows&double_stack=0";
    query += "&ac_id=1";
    query += "&ip=" + ip;
    query += "&info=" + encrypted_info;
    query += "&chksum=" + chksum;
    query += "&n=200";
    query += "&type=1";

    string loginResponse = httpRequest.get(encode_url_params(query));
    cout<< "loginRequests: " << encode_url_params(query) << endl;
    cout << "loginResponse: " << loginResponse;

    return 0;
}