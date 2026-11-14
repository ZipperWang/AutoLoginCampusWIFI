use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    net::IpAddr,
    time::Duration,
};

use base64::{Engine, engine::general_purpose::STANDARD};
pub use macaddr::MacAddr6 as Mac;
use reqwest::{Client, Method, RequestBuilder, Url, redirect::Policy};
use scraper::{Html, Selector};
use thiserror::Error;

use crate::Portal;

const ORIGIN: &str = "https://service.nwafu.edu.cn/";

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("无法读取学校自助服务，请检查校园网连接")]
    Network,
    #[error("自助服务返回 HTTP {0}")]
    Http(u16),
    #[error("自助服务会话未建立或已过期，请使用学号重新登录")]
    Session,
    #[error("自助页面结构无法识别，请在学校自助页核对")]
    Page,
    #[error("自助页拒绝登录，请检查学号、密码和验证码")]
    Login,
    #[error("未找到该账号下与目标匹配的在线记录")]
    Missing,
    #[error("已确认下线 {confirmed}/{total} 条，{reason}。请重新查询设备列表")]
    Partial {
        confirmed: usize,
        total: usize,
        reason: String,
    },
    #[error("操作结果待确认，请重新查询当前状态")]
    Unconfirmed,
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("固定选择器有效")
}

fn csrf(document: &Html) -> Result<(String, String), ServiceError> {
    let field = document
        .select(&selector("meta[name=csrf-param]"))
        .next()
        .and_then(|e| e.attr("content"))
        .filter(|s| s.starts_with("_csrf"));
    let token = document
        .select(&selector("meta[name=csrf-token]"))
        .next()
        .and_then(|e| e.attr("content"))
        .filter(|s| !s.is_empty());
    field
        .zip(token)
        .map(|(f, t)| (f.to_owned(), t.to_owned()))
        .ok_or(ServiceError::Page)
}

pub struct Device {
    pub id: u64,
    pub mac: Option<Mac>,
    pub ip: Option<IpAddr>,
    action: Url,
    csrf: (String, String),
}

#[derive(Clone, Copy)]
pub enum Target {
    Mac(Mac),
    Record(u64),
}

pub struct LoginForm {
    csrf: (String, String),
    pub captcha_png: Option<Vec<u8>>,
}

pub struct SelfService {
    client: Client,
    origin: Url,
}

impl SelfService {
    pub fn new() -> Result<Self, ServiceError> {
        let origin = Url::parse(ORIGIN).expect("固定服务地址有效");
        Self::at(origin)
    }

    fn at(origin: Url) -> Result<Self, ServiceError> {
        let trusted = origin.origin();
        let client = Client::builder()
            .https_only(origin.scheme() == "https")
            .no_proxy()
            .cookie_store(true)
            .redirect(Policy::custom(move |attempt| {
                if !matches!(
                    attempt.url().path(),
                    "/" | "/home" | "/home/index" | "/user/mac-auth" | "/site/login"
                ) {
                    return attempt.stop();
                }
                if attempt.url().origin() == trusted
                    && attempt.previous().len() < 6
                    && attempt.url().username().is_empty()
                    && attempt.url().password().is_none()
                {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("NWAFU-Campus-CLI/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| ServiceError::Network)?;
        Ok(Self { client, origin })
    }

    fn link(&self, href: &str) -> Result<Url, ServiceError> {
        let url = self.origin.join(href).map_err(|_| ServiceError::Page)?;
        if url.origin() != self.origin.origin()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(ServiceError::Page);
        }
        Ok(url)
    }

    async fn body(&self, request: RequestBuilder) -> Result<Vec<u8>, ServiceError> {
        let mut response = request
            .header("Cache-Control", "no-store")
            .send()
            .await
            .map_err(|_| ServiceError::Network)?;
        if !response.status().is_success() {
            return Err(ServiceError::Http(response.status().as_u16()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| ServiceError::Network)? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(ServiceError::Page);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    async fn page(&self, request: RequestBuilder, signed_in: bool) -> Result<Html, ServiceError> {
        let data = self.body(request).await?;
        let doc = Html::parse_document(std::str::from_utf8(&data).map_err(|_| ServiceError::Page)?);
        if signed_in
            && (doc.select(&selector("#login-form")).next().is_some()
                || doc.select(&selector("a#logout")).next().is_none())
        {
            return Err(ServiceError::Session);
        }
        Ok(doc)
    }

    pub async fn sign_in_online(&self) -> Result<(), ServiceError> {
        let portal = Portal::connect().await.map_err(|_| ServiceError::Session)?;
        let username = portal
            .online_username()
            .await
            .map_err(|_| ServiceError::Session)?;
        let data = STANDARD.encode(format!("zh-CN:{username}"));
        let request = self
            .client
            .get(self.link("/site/sso")?)
            .query(&[("data", data)]);
        self.page(request, true).await?;
        Ok(())
    }

    pub async fn login_form(&self) -> Result<LoginForm, ServiceError> {
        let doc = self
            .page(self.client.get(self.origin.clone()), false)
            .await?;
        let form = doc
            .select(&selector("form#login-form[method=post]"))
            .next()
            .ok_or(ServiceError::Page)?;
        if self.link(form.attr("action").unwrap_or("/"))? != self.origin {
            return Err(ServiceError::Page);
        }
        let captcha = form
            .select(&selector("input[name='LoginForm[verifyCode]']"))
            .next()
            .is_some();
        let captcha_png = if captcha {
            let img = form
                .select(&selector("img#loginform-verifycode-image"))
                .next()
                .and_then(|img| img.attr("src"))
                .ok_or(ServiceError::Page)?;
            let url = self.link(img)?;
            if url.path() != "/site/captcha" {
                return Err(ServiceError::Page);
            }
            let bytes = self.body(self.client.get(url)).await?;
            if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err(ServiceError::Page);
            }
            Some(bytes)
        } else {
            None
        };
        Ok(LoginForm {
            csrf: csrf(&doc)?,
            captcha_png,
        })
    }

    pub async fn sign_in(
        &self,
        form: LoginForm,
        username: &str,
        password: &str,
        code: &str,
    ) -> Result<(), ServiceError> {
        let request = self
            .client
            .post(self.origin.clone())
            .header("Referer", self.origin.as_str())
            .form(&[
                (form.csrf.0.as_str(), form.csrf.1.as_str()),
                ("LoginForm[username]", username),
                ("LoginForm[password]", password),
                ("LoginForm[verifyCode]", code),
            ]);
        self.page(request, true)
            .await
            .map(|_| ())
            .map_err(|e| match e {
                ServiceError::Session => ServiceError::Login,
                e => e,
            })
    }

    pub async fn devices(&self) -> Result<Vec<Device>, ServiceError> {
        let mut url = self.link("/home")?;
        let mut visited = HashSet::new();
        let mut ids = HashMap::new();
        let mut records = Vec::<Device>::new();
        loop {
            if visited.len() >= 64 || !visited.insert(url.clone()) {
                return Err(ServiceError::Page);
            }
            let doc = self.page(self.client.get(url.clone()), true).await?;
            let grid = doc
                .select(&selector(".query-online"))
                .next()
                .ok_or(ServiceError::Page)?;
            let table = grid
                .select(&selector("table"))
                .next()
                .ok_or(ServiceError::Page)?;
            let token = csrf(&doc)?;
            for row in table.select(&selector("tbody tr")) {
                if row.select(&selector(".empty")).next().is_some() {
                    continue;
                }
                let action = row
                    .select(&selector("a[data-method=post][href]"))
                    .filter_map(|a| self.link(a.attr("href")?).ok())
                    .find(|u| u.path() == "/home/delete")
                    .ok_or(ServiceError::Page)?;
                let mut id = None;
                let mut raw_mac = None;
                for (k, v) in action.query_pairs() {
                    match k.as_ref() {
                        "id" if id.is_none() => {
                            id = Some(v.parse::<u64>().map_err(|_| ServiceError::Page)?)
                        }
                        "user_mac" if raw_mac.is_none() => raw_mac = Some(v.into_owned()),
                        _ => return Err(ServiceError::Page),
                    }
                }
                let id = id.filter(|id| *id > 0).ok_or(ServiceError::Page)?;
                if row.attr("data-key").and_then(|s| s.parse::<u64>().ok()) != Some(id) {
                    return Err(ServiceError::Page);
                }
                let raw_mac = raw_mac.ok_or(ServiceError::Page)?;
                let mac = if raw_mac.is_empty() {
                    None
                } else {
                    Some(raw_mac.parse::<Mac>().map_err(|_| ServiceError::Page)?)
                };
                let ip = row
                    .select(&selector("[data-col-seq='1']"))
                    .next()
                    .and_then(|e| e.text().collect::<String>().trim().parse().ok());
                if let Some(previous) = ids.insert(id, mac) {
                    if previous != mac {
                        return Err(ServiceError::Page);
                    }
                    continue;
                }
                records.push(Device {
                    id,
                    mac,
                    ip,
                    action,
                    csrf: token.clone(),
                });
            }
            let next = grid
                .select(&selector(
                    "li.next:not(.disabled) a[href], a[rel=next][href]",
                ))
                .next()
                .and_then(|a| a.attr("href"));
            let Some(next) = next else {
                break;
            };
            url = url.join(next).map_err(|_| ServiceError::Page)?;
            if url.origin() != self.origin.origin() || url.path() != "/home" {
                return Err(ServiceError::Page);
            }
        }
        Ok(records)
    }

    pub async fn disconnect(&self, target: Target) -> Result<usize, ServiceError> {
        let mut current = self.devices().await?;
        let selected: Vec<_> = current
            .iter()
            .filter(|r| match target {
                Target::Mac(m) => r.mac == Some(m),
                Target::Record(id) => r.id == id,
            })
            .map(|r| (r.id, r.mac))
            .collect();
        if selected.is_empty() {
            return Err(ServiceError::Missing);
        }
        let total = selected.len();
        for (confirmed, &(id, mac)) in selected.iter().enumerate() {
            let Some(record) = current.iter().find(|r| r.id == id) else {
                continue;
            };
            let partial = |reason: &dyn Display| ServiceError::Partial {
                confirmed,
                total,
                reason: reason.to_string(),
            };
            if record.mac != mac {
                return Err(partial(&"记录对应的设备已变化"));
            }
            let req = self
                .client
                .post(record.action.clone())
                .header("Referer", self.link("/home")?.as_str())
                .form(&[(&record.csrf.0, &record.csrf.1)]);
            let sent = self.page(req, true).await;
            current = self.devices().await.map_err(|e| partial(&e))?;
            if current.iter().any(|r| r.id == id && r.mac == mac) {
                return Err(partial(&sent.err().unwrap_or(ServiceError::Unconfirmed)));
            }
        }
        if let Target::Mac(mac) = target
            && current.iter().any(|r| r.mac == Some(mac))
        {
            return Err(ServiceError::Partial {
                confirmed: total,
                total,
                reason: "该设备出现了新的在线记录".into(),
            });
        }
        Ok(total)
    }

    async fn switch_page(&self) -> Result<(bool, RequestBuilder), ServiceError> {
        let page = self.link("/user/mac-auth")?;
        let doc = self.page(self.client.get(page.clone()), true).await?;
        let anchors = selector("a[href]");
        let mut controls = doc.select(&anchors).filter_map(|a| {
            let url = self.link(a.attr("href")?).ok()?;
            if !matches!(url.path(), "/user/open" | "/user/close") || url.query().is_some() {
                return None;
            }
            Some((a, url))
        });
        let (button, url) = controls.next().ok_or(ServiceError::Page)?;
        if controls.next().is_some() {
            return Err(ServiceError::Page);
        }
        let label = button.text().collect::<String>();
        let enabled = match label.trim().to_ascii_lowercase().as_str() {
            "关闭" | "关闭无感知认证" | "close" | "disable" => true,
            "开"
            | "开启"
            | "开通"
            | "打开"
            | "启用"
            | "开启无感知"
            | "开启无感知认证"
            | "open"
            | "enable" => false,
            _ => return Err(ServiceError::Page),
        };
        let method = button.attr("data-method").unwrap_or("get");
        let request = if method.eq_ignore_ascii_case("get") {
            self.client.request(Method::GET, url)
        } else if method.eq_ignore_ascii_case("post") {
            let (field, token) = csrf(&doc)?;
            self.client.post(url).form(&[(field, token)])
        } else {
            return Err(ServiceError::Page);
        };
        Ok((enabled, request.header("Referer", page.as_str())))
    }

    pub async fn seamless(&self, desired: Option<bool>) -> Result<bool, ServiceError> {
        let (current, request) = self.switch_page().await?;
        let Some(desired) = desired.filter(|wanted| *wanted != current) else {
            return Ok(current);
        };
        let sent = self.page(request, true).await;
        let (actual, _) = self
            .switch_page()
            .await
            .map_err(|_| ServiceError::Unconfirmed)?;
        if actual == desired {
            Ok(actual)
        } else {
            Err(sent.err().unwrap_or(ServiceError::Unconfirmed))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        io::{BufRead, BufReader, Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };

    #[derive(Debug)]
    struct Seen {
        method: String,
        url: Url,
        cookie: bool,
        body: String,
    }

    struct Mock {
        service: SelfService,
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<Vec<Seen>>>,
    }

    impl Mock {
        fn new(mut respond: impl FnMut(&Seen) -> String + Send + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin =
                Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let stopping = stop.clone();
            let base = origin.clone();
            let thread = thread::spawn(move || {
                let mut seen = Vec::new();
                for connection in listener.incoming() {
                    let mut stream = connection.unwrap();
                    if stopping.load(Ordering::Acquire) {
                        break;
                    }
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut reader = BufReader::new(&stream);
                    let mut head = String::new();
                    reader.read_line(&mut head).unwrap();
                    let mut parts = head.split_whitespace();
                    let method = parts.next().unwrap().to_string();
                    let url = base.join(parts.next().unwrap()).unwrap();
                    let mut len = 0;
                    let mut cookie = false;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some((k, v)) = line.split_once(':') {
                            if k.eq_ignore_ascii_case("content-length") {
                                len = v.trim().parse().unwrap();
                            }
                            if k.eq_ignore_ascii_case("cookie") {
                                cookie = v.contains("session=fixture");
                            }
                        }
                    }
                    let mut body = vec![0; len];
                    reader.read_exact(&mut body).unwrap();
                    let req = Seen {
                        method,
                        url,
                        cookie,
                        body: String::from_utf8(body).unwrap(),
                    };
                    let html = respond(&req);
                    seen.push(req);
                    write!(stream,"HTTP/1.1 200 OK\r\nConnection: close\r\nSet-Cookie: session=fixture; Path=/; HttpOnly\r\nContent-Length: {}\r\n\r\n{html}",html.len()).unwrap();
                }
                seen
            });
            Self {
                service: SelfService::at(origin).unwrap(),
                stop,
                thread: Some(thread),
            }
        }

        fn finish(&mut self) -> Vec<Seen> {
            self.stop.store(true, Ordering::Release);
            let url = &self.service.origin;
            drop(TcpStream::connect((url.host_str().unwrap(), url.port().unwrap())).unwrap());
            self.thread.take().unwrap().join().unwrap()
        }
    }

    impl Drop for Mock {
        fn drop(&mut self) {
            if self.thread.is_some() {
                self.stop.store(true, Ordering::Release);
                let url = &self.service.origin;
                let _ = TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()));
            }
        }
    }

    const META: &str = r#"<a id="logout"></a><meta name="csrf-param" content="_csrf-8800"><meta name="csrf-token" content="fixture-token">"#;
    const MAC: &str = "02:00:00:00:00:01";
    const OTHER: &str = "02:00:00:00:00:02";

    fn home(records: &BTreeMap<u64, &str>, next: bool) -> String {
        let mut html = format!("{META}<section class='query-online'><table><tbody>");
        for (id, mac) in records {
            html.push_str(&format!("<tr data-key='{id}'><td data-col-seq='1'>192.0.2.1</td><td><a data-method='post' href='/home/delete?id={id}&amp;user_mac={mac}'>下线</a></td></tr>"));
        }
        html.push_str("</tbody></table>");
        if next {
            html.push_str("<ul><li class='next'><a href='/home?page=2'>下一页</a></li></ul>");
        }
        html.push_str("</section>");
        html
    }

    #[tokio::test]
    async fn password_login_submits_the_form_csrf_and_human_answer_in_the_same_session() {
        let mut mock = Mock::new(|req| {
            if req.method == "GET" {
                return format!("{META}<form id='login-form' action='/' method='post'></form>");
            }
            assert_eq!(req.method, "POST");
            assert_eq!(req.url.path(), "/");
            assert!(req.cookie);
            let body = Url::parse(&format!("http://localhost/?{}", req.body)).unwrap();
            let fields: HashMap<_, _> = body.query_pairs().collect();
            assert_eq!(fields["_csrf-8800"], "fixture-token");
            assert_eq!(fields["LoginForm[username]"], "demo+@test");
            assert_eq!(fields["LoginForm[password]"], "demo &+=密码");
            assert_eq!(fields["LoginForm[verifyCode]"], "fixture-answer");
            META.into()
        });
        let form = mock.service.login_form().await.unwrap();
        mock.service
            .sign_in(form, "demo+@test", "demo &+=密码", "fixture-answer")
            .await
            .unwrap();
        assert_eq!(mock.finish().len(), 2);
    }

    #[tokio::test]
    async fn disconnects_only_selected_records_and_verifies_each_write() {
        for whole_device in [true, false] {
            let mut records = BTreeMap::from([(11, MAC), (12, MAC), (13, OTHER)]);
            let mut mock = Mock::new(move |r| match r.url.path() {
                "/home" => home(&records, false),
                "/home/delete" => {
                    assert_eq!(r.method, "POST");
                    assert!(r.cookie);
                    assert_eq!(r.body, "_csrf-8800=fixture-token");
                    let fields: HashMap<_, _> = r.url.query_pairs().collect();
                    assert_eq!(&fields["user_mac"], MAC);
                    records.remove(&fields["id"].parse::<u64>().unwrap());
                    META.into()
                }
                _ => panic!("未知测试路径"),
            });
            let target = if whole_device {
                Target::Mac(MAC.parse().unwrap())
            } else {
                Target::Record(11)
            };
            let count = mock.service.disconnect(target).await.unwrap();
            assert_eq!(count, if whole_device { 2 } else { 1 });
            let history = mock.finish();
            assert_eq!(history.len(), count * 2 + 1);
            assert_eq!(history.last().unwrap().url.path(), "/home");
        }
    }

    #[tokio::test]
    async fn reports_partial_failure_without_touching_other_devices() {
        let mut records = BTreeMap::from([(11, MAC), (12, MAC), (13, OTHER)]);
        let mut mock = Mock::new(move |r| {
            if r.url.path() == "/home/delete"
                && r.url.query_pairs().any(|(k, v)| k == "id" && v == "11")
            {
                records.remove(&11);
            }
            home(&records, false)
        });
        assert!(matches!(
            mock.service
                .disconnect(Target::Mac(MAC.parse().unwrap()))
                .await,
            Err(ServiceError::Partial {
                confirmed: 1,
                total: 2,
                ..
            })
        ));
        assert!(matches!(
            mock.service.disconnect(Target::Record(99)).await,
            Err(ServiceError::Missing)
        ));
        let history = mock.finish();
        assert_eq!(history.iter().filter(|r| r.method == "POST").count(), 2);
        assert!(
            !history
                .iter()
                .any(|r| r.url.query_pairs().any(|(k, v)| k == "id" && v == "13"))
        );
    }

    #[tokio::test]
    async fn handles_pagination_and_rejects_expired_sessions() {
        let mut mock = Mock::new(|r| {
            if r.url.query().is_none() {
                home(&BTreeMap::from([(11, MAC)]), true)
            } else {
                home(&BTreeMap::from([(12, OTHER)]), false)
            }
        });
        assert_eq!(mock.service.devices().await.unwrap().len(), 2);
        assert_eq!(mock.finish().len(), 2);
        let mut mock = Mock::new(|_| "<form id='login-form'></form>".into());
        assert!(matches!(
            mock.service.disconnect(Target::Record(11)).await,
            Err(ServiceError::Session)
        ));
        assert_eq!(mock.finish().len(), 1);
        let mut mock = Mock::new(|_| {
            home(&BTreeMap::from([(11, MAC)]), false)
                .replace("/home/delete", "https://other.invalid/home/delete")
        });
        assert!(matches!(
            mock.service.disconnect(Target::Record(11)).await,
            Err(ServiceError::Page)
        ));
        assert_eq!(mock.finish().len(), 1);
    }

    #[tokio::test]
    async fn reads_sets_and_verifies_the_account_switch_without_repeating_a_toggle() {
        let mut enabled = true;
        let mut mock = Mock::new(move |r| {
            if r.url.path() == "/user/open" {
                assert!(r.cookie);
                assert_eq!(r.method, "GET");
                enabled = !enabled;
            } else {
                assert_eq!(r.url.path(), "/user/mac-auth");
            }
            format!(
                "{META}<a href='/user/open'>{}</a>",
                if enabled { "关闭" } else { "开启" }
            )
        });
        assert!(mock.service.seamless(None).await.unwrap());
        assert!(mock.service.seamless(Some(true)).await.unwrap());
        assert!(!mock.service.seamless(Some(false)).await.unwrap());
        assert!(!mock.service.seamless(Some(false)).await.unwrap());
        assert!(mock.service.seamless(Some(true)).await.unwrap());
        let history = mock.finish();
        assert_eq!(
            history
                .iter()
                .filter(|r| r.url.path() == "/user/open")
                .count(),
            2
        );
        let mut mock = Mock::new(|_| format!("{META}<a href='/user/open'>关闭</a>"));
        assert!(matches!(
            mock.service.seamless(Some(false)).await,
            Err(ServiceError::Unconfirmed)
        ));
        assert_eq!(mock.finish().len(), 3);
    }
}
