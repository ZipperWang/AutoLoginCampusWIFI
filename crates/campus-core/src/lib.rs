mod protocol;
pub mod self_service;

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{Client, Url, redirect::Policy};
use serde::Deserialize;
use thiserror::Error;
use tokio::{net::lookup_host, time::timeout};

const HOST: &str = "portal.nwafu.edu.cn";
const ORIGIN: &str = "https://portal.nwafu.edu.cn";
const CALLBACK: &str = "campus";
const AC_ID: &str = "1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    Online,
    Offline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginOutcome {
    Authenticated,
    AlreadyOnline,
}

#[derive(Debug, Error)]
pub enum PortalError {
    #[error("学校门户网络不可用，认证状态待确认")]
    Network,
    #[error("学校门户返回 HTTP {0}")]
    Http(u16),
    #[error("学校门户响应格式异常")]
    Format,
    #[error("门户状态待确认")]
    Unknown,
    #[error("网络路由已变化，需要重新检测")]
    RouteChanged,
    #[error("学校门户拒绝认证（{0}）")]
    Rejected(String),
}

pub struct Portal {
    client: Client,
    origin: String,
    peer: SocketAddr,
    local: IpAddr,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Reply {
    error: String,
    error_msg: String,
    res: String,
    suc_msg: String,
    ecode: serde_json::Value,
    challenge: String,
    client_ip: String,
    user_name: String,
}

fn route_to(peer: SocketAddr) -> Result<IpAddr, PortalError> {
    let any = if peer.is_ipv4() {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    };
    let socket = UdpSocket::bind((any, 0)).map_err(|_| PortalError::Network)?;
    socket.connect(peer).map_err(|_| PortalError::Network)?;
    Ok(socket.local_addr().map_err(|_| PortalError::Network)?.ip())
}

impl Portal {
    pub async fn connect() -> Result<Self, PortalError> {
        let mut addresses: Vec<_> = timeout(Duration::from_secs(5), lookup_host((HOST, 443)))
            .await
            .map_err(|_| PortalError::Network)?
            .map_err(|_| PortalError::Network)?
            .collect();
        addresses.sort_by_key(SocketAddr::is_ipv6);
        for peer in addresses {
            if let Ok(local) = route_to(peer) {
                let client = Client::builder()
                    .https_only(true)
                    .no_proxy()
                    .redirect(Policy::none())
                    .local_address(local)
                    .resolve(HOST, peer)
                    .timeout(Duration::from_secs(8))
                    .connect_timeout(Duration::from_secs(4))
                    .user_agent(concat!("NWAFU-Campus-CLI/", env!("CARGO_PKG_VERSION")))
                    .build()
                    .map_err(|_| PortalError::Network)?;
                return Ok(Self {
                    client,
                    origin: ORIGIN.to_owned(),
                    peer,
                    local,
                });
            }
        }
        Err(PortalError::Network)
    }

    pub fn local_ip(&self) -> IpAddr {
        self.local
    }

    pub fn private_destination(&self) -> bool {
        match self.peer.ip() {
            IpAddr::V4(ip) => ip.is_private(),
            IpAddr::V6(ip) => ip.is_unique_local(),
        }
    }

    pub async fn captive_redirect(&self) -> bool {
        let Ok(client) = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .local_address(self.local)
            .timeout(Duration::from_secs(4))
            .build()
        else {
            return false;
        };
        let Ok(response) = client
            .get("http://www.msftconnecttest.com/connecttest.txt")
            .send()
            .await
        else {
            return false;
        };
        response.status().is_redirection()
            && response
                .headers()
                .get("Location")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| response.url().join(s).ok())
                .is_some_and(|url| {
                    matches!(url.scheme(), "http" | "https") && url.host_str() == Some(HOST)
                })
    }

    async fn request(&self, path: &str, fields: &[(&str, &str)]) -> Result<Reply, PortalError> {
        if route_to(self.peer)? != self.local {
            return Err(PortalError::RouteChanged);
        }
        let mut url = Url::parse(&self.origin).map_err(|_| PortalError::Format)?;
        url.set_path(path);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_string();
        url.query_pairs_mut()
            .extend_pairs(fields.iter().copied())
            .append_pair("callback", CALLBACK)
            .append_pair("_", &stamp);
        let mut response = self
            .client
            .get(url)
            .header("Referer", &self.origin)
            .header("Cache-Control", "no-store")
            .send()
            .await
            .map_err(|_| PortalError::Network)?;
        if !response.status().is_success() {
            return Err(PortalError::Http(response.status().as_u16()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| PortalError::Network)? {
            if bytes.len() + chunk.len() > 64 * 1024 {
                return Err(PortalError::Format);
            }
            bytes.extend_from_slice(&chunk);
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| PortalError::Format)?
            .trim();
        let json = if text.starts_with('{') {
            text
        } else {
            text.trim_end_matches(';')
                .trim_end()
                .strip_prefix(CALLBACK)
                .and_then(|s| s.trim_start().strip_prefix('('))
                .and_then(|s| s.strip_suffix(')'))
                .ok_or(PortalError::Format)?
        };
        serde_json::from_str(json).map_err(|_| PortalError::Format)
    }

    pub async fn status(&self) -> Result<Session, PortalError> {
        let reply = self.request("/cgi-bin/rad_user_info", &[]).await?;
        if reply.error == "ok" && !reply.user_name.is_empty() {
            return Ok(Session::Online);
        }
        if [&reply.error, &reply.res, &reply.error_msg]
            .iter()
            .any(|code| matches!(code.as_str(), "not_online" | "not_online_error"))
        {
            return Ok(Session::Offline);
        }
        Err(PortalError::Unknown)
    }

    pub async fn online_username(&self) -> Result<String, PortalError> {
        let reply = self.request("/cgi-bin/rad_user_info", &[]).await?;
        if reply.error == "ok" && !reply.user_name.trim().is_empty() {
            Ok(reply.user_name)
        } else {
            Err(PortalError::Unknown)
        }
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<LoginOutcome, PortalError> {
        let challenge = self
            .request(
                "/cgi-bin/get_challenge",
                &[("username", username), ("ip", "")],
            )
            .await?;
        let token = &challenge.challenge;
        if (!challenge.error.is_empty() && challenge.error != "ok")
            || token.is_empty()
            || !token.is_ascii()
            || token.len() > 256
            || challenge.client_ip.parse::<IpAddr>().is_err()
        {
            return Err(PortalError::Format);
        }
        let ip = &challenge.client_ip;
        let (hash, info, checksum) = protocol::login_fields(username, password, ip, token, AC_ID);
        let os = match std::env::consts::OS {
            "windows" => "Windows",
            "macos" => "Mac OS",
            _ => "Linux",
        };
        let reply = self
            .request(
                "/cgi-bin/srun_portal",
                &[
                    ("action", "login"),
                    ("username", username),
                    ("password", &hash),
                    ("os", os),
                    ("name", os),
                    ("double_stack", "0"),
                    ("ac_id", AC_ID),
                    ("ip", ip),
                    ("info", &info),
                    ("chksum", &checksum),
                    ("n", "200"),
                    ("type", "1"),
                ],
            )
            .await?;
        if [&reply.error, &reply.error_msg, &reply.suc_msg]
            .iter()
            .any(|v| v.as_str() == "ip_already_online_error")
        {
            return match self.status().await? {
                Session::Online => Ok(LoginOutcome::AlreadyOnline),
                Session::Offline => Err(PortalError::Unknown),
            };
        }
        if reply.error == "ok" {
            return Ok(LoginOutcome::Authenticated);
        }
        let codes = [
            reply.ecode.as_str().unwrap_or(""),
            &reply.error_msg,
            &reply.error,
            &reply.res,
        ];
        if codes.iter().any(|code| {
            matches!(
                code.split(':').next().unwrap_or(""),
                "E2532"
                    | "challenge_expired"
                    | "challenge_error"
                    | "no_response_data_error"
                    | "sign_error"
            )
        }) {
            return Err(PortalError::Unknown);
        }
        if codes.iter().all(|code| code.is_empty()) {
            return Err(PortalError::Format);
        }
        let safe_code = codes
            .iter()
            .flat_map(|s| s.split(|c: char| !c.is_ascii_alphanumeric()))
            .find(|s| {
                s.len() == 5 && s.starts_with('E') && s[1..].bytes().all(|b| b.is_ascii_digit())
            })
            .unwrap_or("登录请求未获接受");
        Err(PortalError::Rejected(safe_code.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn matches_portal_javascript() {
        for (user, pass, md5, checksum) in [
            (
                "2026000001",
                "demo+A&B=",
                "994550de832d1db397e93b26b1033b1a",
                "2a26692135f54e27454110152413788507d3fe86",
            ),
            (
                "students+lab@example",
                "演示🔑\"\\ &+=",
                "1a201ceca264cca2bd43bd4f3b7967e0",
                "a9194f0d30dc356acf3e31cc03afb5c32a1b0bb1",
            ),
        ] {
            let (hash, _, actual) = protocol::login_fields(
                user,
                pass,
                "192.0.2.17",
                "0123456789abcdef0123456789abcdef",
                "1",
            );
            assert_eq!(hash, format!("{{MD5}}{md5}"));
            assert_eq!(actual, checksum);
        }
    }

    fn mock(replies: Vec<&'static str>) -> (Portal, thread::JoinHandle<Vec<Url>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let thread = thread::spawn(move || {
            let mut requests = Vec::new();
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let target = line.split_whitespace().nth(1).unwrap();
                requests.push(Url::parse(&format!("http://localhost{target}")).unwrap());
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .unwrap();
            }
            requests
        });
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        (
            Portal {
                client,
                origin: format!("http://{peer}"),
                peer,
                local: peer.ip(),
            },
            thread,
        )
    }

    #[tokio::test]
    async fn login_uses_challenge_ip_and_encodes_each_field_once() {
        let (portal, server) = mock(vec![
            r#"campus({"error":"not_online_error"});"#,
            r#"campus({"error":"ok","challenge":"0123456789abcdef0123456789abcdef","client_ip":"192.0.2.17"})"#,
            r#"campus({"error":"ok","suc_msg":"login_ok"})"#,
            r#"{"error":"ok","user_name":"demo"}"#,
        ]);
        assert_eq!(portal.status().await.unwrap(), Session::Offline);
        portal
            .login("students+lab@example", "演示🔑\"\\ &+=")
            .await
            .unwrap();
        assert_eq!(portal.status().await.unwrap(), Session::Online);
        let requests = server.join().unwrap();
        assert_eq!(requests[1].path(), "/cgi-bin/get_challenge");
        assert_eq!(requests[2].path(), "/cgi-bin/srun_portal");
        let fields: HashMap<_, _> = requests[2].query_pairs().collect();
        assert_eq!(fields["username"], "students+lab@example");
        assert_eq!(fields["ip"], "192.0.2.17");
        assert_eq!(fields["chksum"], "a9194f0d30dc356acf3e31cc03afb5c32a1b0bb1");
        assert_eq!(
            fields["info"],
            "{SRBX1}NCgFgm9phNQjo24Ru1CHTAvTPPrjJvaso/odpb1JKhfgiI2CiQREWcafxUBLq2jxvAf8EALwURGYHbJvzc4H0xFnHJRNyRiC6YgUufTW9M1W0OPq0uwOMxp5pqcwQj4390buRRsubeR5zdDvzoOCbvGA6hH="
        );
    }

    #[tokio::test]
    async fn distinguishes_unknown_offline_and_invalid_jsonp() {
        let (portal, server) = mock(vec![
            r#"{"error":"ok"}"#,
            r#"{"error":"server_error"}"#,
            r#"other({"error":"not_online_error"})"#,
            r#"campus({"error":"not_online_error"})"#,
        ]);
        assert!(matches!(portal.status().await, Err(PortalError::Unknown)));
        assert!(matches!(portal.status().await, Err(PortalError::Unknown)));
        assert!(matches!(portal.status().await, Err(PortalError::Format)));
        assert_eq!(portal.status().await.unwrap(), Session::Offline);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn rejects_bad_credentials_and_confirms_existing_sessions() {
        let challenge =
            r#"{"challenge":"0123456789abcdef0123456789abcdef","client_ip":"192.0.2.17"}"#;
        let (portal, server) = mock(vec![
            challenge,
            r#"{"error":"login_error","ecode":"E2553"}"#,
        ]);
        assert!(matches!(
            portal.login("demo", "fixture").await,
            Err(PortalError::Rejected(code)) if code == "E2553"
        ));
        server.join().unwrap();
        let (portal, server) = mock(vec![
            challenge,
            r#"{"error_msg":"ip_already_online_error"}"#,
            r#"{"error":"ok","user_name":"demo"}"#,
        ]);
        assert_eq!(
            portal.login("demo", "fixture").await.unwrap(),
            LoginOutcome::AlreadyOnline
        );
        server.join().unwrap();
    }
}
