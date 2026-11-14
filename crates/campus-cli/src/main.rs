mod self_service;

use std::{
    io::{self, IsTerminal, Write},
    process::ExitCode,
    time::Duration,
};

use campus_core::{LoginOutcome, Portal, PortalError, Session};
use campus_platform::{
    CredentialError, IfWatcher, campus_wifi, credential, interface_for, lock_login,
};
use clap::{Args, Parser, Subcommand};
use futures_util::StreamExt;
use tokio::time::{Instant, sleep, sleep_until};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "campus", version, about = "西北农林科技大学校园网认证客户端")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "管理自助服务中的在线设备和无感知认证")]
    Service(self_service::ServiceArgs),
    #[command(about = "查询当前连接的校园网认证状态")]
    Status,
    #[command(about = "在当前校园网络上登录一次")]
    Login(Account),
    #[command(about = "在前台监听网络变化并按需登录")]
    Watch {
        #[command(flatten)]
        account: Account,
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(5..=3600), help = "例行复查间隔，单位为秒")]
        interval: u64,
    },
}

#[derive(Args)]
struct Account {
    #[arg(short, long, value_name = "学号", help = "校园网账号，交互终端可留空")]
    username: Option<String>,
    #[arg(long, help = "从标准输入读取一行密码")]
    password_stdin: bool,
    #[arg(long, help = "认证成功后把密码存入系统凭据库")]
    remember: bool,
    #[arg(long, help = "认证前主动连接开放无线网络NWAFU")]
    connect_wifi: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let command = Cli::parse().command;
    let result = tokio::select! {
        result = run(command) => result,
        signal = tokio::signal::ctrl_c() => signal.map(|_| 0).map_err(|_| (1, "无法监听退出信号".into())),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err((code, message)) => {
            eprintln!("{message}");
            ExitCode::from(code)
        }
    }
}

async fn run(command: Command) -> Result<u8, (u8, String)> {
    let (mut options, interval) = match command {
        Command::Service(args) => return self_service::run(args).await,
        Command::Status => {
            let portal = Portal::connect().await.map_err(|e| (4, e.to_string()))?;
            let ip = portal.local_ip();
            println!(
                "当前接口：{}（{ip}）",
                interface_for(ip)
                    .map(|i| i.name)
                    .as_deref()
                    .unwrap_or("系统默认路由")
            );
            return match portal.status().await {
                Ok(Session::Online) => {
                    println!("学校门户：已在线");
                    Ok(0)
                }
                Ok(Session::Offline) => {
                    println!("学校门户：尚未认证");
                    Ok(3)
                }
                Err(error) => Err((4, error.to_string())),
            };
        }
        Command::Login(account) => (account, None),
        Command::Watch { account, interval } => (account, Some(Duration::from_secs(interval))),
    };

    if options.password_stdin && options.username.is_none() {
        return Err((2, "--password-stdin 需要同时指定 --username".into()));
    }
    let mut password: Option<Zeroizing<String>> = None;
    if let Some(username) = &mut options.username {
        *username = username.trim().to_owned();
        if username.is_empty() {
            return Err((2, "请输入有效学号".into()));
        }
    }
    let mut watcher = None;
    if interval.is_some() {
        match IfWatcher::new() {
            Ok(stream) => watcher = Some(stream),
            Err(_) => eprintln!("系统地址通知不可用，使用定时复查"),
        }
    }
    let mut failures = 0u32;
    let mut last_message = String::new();
    loop {
        let attempt: Result<String, (u8, String)> = async {
            let wifi_index = if options.connect_wifi {
                let mut index = tokio::task::spawn_blocking(|| campus_wifi(true))
                    .await
                    .map_err(|_| (4, "无线网络操作中断".into()))?
                    .map_err(|e| (4, format!("连接 NWAFU 失败（{e}）")))?;
                let end = Instant::now() + Duration::from_secs(20);
                while index.is_none() {
                    if Instant::now() >= end {
                        return Err((4, "等待 NWAFU 连接超时，请检查无线网络或定位权限".into()));
                    }
                    sleep(Duration::from_secs(1)).await;
                    index = tokio::task::spawn_blocking(|| campus_wifi(false))
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .flatten();
                }
                index
            } else {
                None
            };

            let deadline = Instant::now() + Duration::from_secs(15);
            let portal = loop {
                match Portal::connect().await {
                    Ok(portal)
                        if wifi_index.is_none()
                            || interface_for(portal.local_ip()).and_then(|i| i.index)
                                == wifi_index =>
                    {
                        break portal;
                    }
                    Err(error) if wifi_index.is_none() => return Err((4, error.to_string())),
                    _ if Instant::now() >= deadline => {
                        return Err((4, "NWAFU 尚未取得通往学校门户的路由".into()));
                    }
                    _ => sleep(Duration::from_secs(1)).await,
                }
            };
            let state = portal.status().await.map_err(|e| (4, e.to_string()))?;
            if state == Session::Online {
                return Ok("学校门户：已在线".into());
            }
            if !portal.private_destination()
                && !tokio::task::spawn_blocking(|| campus_wifi(false))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .flatten()
                    .is_some_and(|index| {
                        interface_for(portal.local_ip()).and_then(|i| i.index) == Some(index)
                    })
                && !portal.captive_redirect().await
            {
                return Err((4, "当前网络未识别为西农校园网，等待接入".into()));
            }

            if password.is_none() {
                if options.username.is_none() {
                    if !io::stdin().is_terminal() {
                        return Err((2, "非交互运行请指定 --username".into()));
                    }
                    eprint!("学号：");
                    io::stderr()
                        .flush()
                        .map_err(|_| (1, "无法显示输入提示".into()))?;
                    let mut username = String::new();
                    io::stdin()
                        .read_line(&mut username)
                        .map_err(|_| (2, "读取学号失败".into()))?;
                    if username.trim().is_empty() {
                        return Err((2, "请输入有效学号".into()));
                    }
                    options.username = Some(username.trim().to_owned());
                }
                let username = options.username.as_deref().unwrap();
                let input = if options.password_stdin {
                    let mut line = Zeroizing::new(String::new());
                    io::stdin()
                        .read_line(&mut line)
                        .map_err(|_| (2, "读取标准输入失败".into()))?;
                    if line.ends_with('\n') {
                        line.pop();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                    }
                    line
                } else {
                    let interactive = io::stdin().is_terminal();
                    let saved = if options.remember && interactive {
                        None
                    } else {
                        let account = username.to_owned();
                        match tokio::task::spawn_blocking(move || {
                            credential(&account)?.get_password()
                        })
                        .await
                        {
                            Ok(Ok(value)) => Some(value),
                            Ok(Err(CredentialError::NoEntry)) => None,
                            _ if interactive => None,
                            _ => {
                                return Err((
                                    5,
                                    "系统凭据库不可用，请使用 --password-stdin".into(),
                                ));
                            }
                        }
                    };
                    match saved {
                        Some(value) => Zeroizing::new(value),
                        None if interactive => Zeroizing::new(
                            rpassword::prompt_password("密码：")
                                .map_err(|_| (2, "读取密码失败".into()))?,
                        ),
                        None => {
                            return Err((5, "找不到已保存的凭据，请使用 --password-stdin".into()));
                        }
                    }
                };
                if input.is_empty() {
                    return Err((2, "密码为空".into()));
                }
                password = Some(input);
            }
            let username = options.username.as_deref().unwrap();
            let secret = password.as_ref().unwrap();
            let _guard = lock_login().map_err(|e| match e.kind() {
                io::ErrorKind::WouldBlock => (6, "另一进程正在认证，稍后复查".into()),
                _ => (1, format!("无法取得用户认证锁（{e}）")),
            })?;
            if portal.status().await.map_err(|e| (4, e.to_string()))? == Session::Online {
                return Ok("学校门户：已在线".into());
            }
            let authenticated = portal.login(username, secret).await.map_err(|e| match e {
                PortalError::Rejected(_) => (5, format!("{e}，请检查账号和密码后重新运行")),
                _ => (4, e.to_string()),
            })?;
            if authenticated == LoginOutcome::AlreadyOnline {
                return Ok("学校门户：已在线".into());
            }
            if options.remember {
                let name = username.to_owned();
                let value = secret.clone();
                tokio::task::spawn_blocking(move || credential(&name)?.set_password(&value))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .ok_or_else(|| (5, "认证已成功，系统凭据库保存失败".to_owned()))?;
                options.remember = false;
            }
            Ok("学校门户：登录成功".into())
        }
        .await;
        let Some(base) = interval else {
            return match attempt {
                Ok(message) => {
                    println!("{message}");
                    Ok(0)
                }
                Err(err) => Err(err),
            };
        };
        let (message, delay) = match attempt {
            Ok(message) => {
                failures = 0;
                (message, base)
            }
            Err((4 | 6, message)) => {
                let delay = Duration::from_secs(5u64 << failures.min(10)).min(base);
                failures = failures.saturating_add(1);
                (message, delay)
            }
            Err(err) => return Err(err),
        };
        if message != last_message {
            println!("{message}");
            last_message = message;
        }
        let mut deadline = Instant::now() + delay;
        loop {
            tokio::select! {
                _ = sleep_until(deadline) => break,
                event = async { watcher.as_mut().unwrap().next().await }, if watcher.is_some() => {
                    match event {
                        Some(Ok(_)) => { deadline = deadline.min(Instant::now() + Duration::from_secs(2)); }
                        _ => { watcher = None; eprintln!("系统地址通知中断，使用定时复查"); }
                    }
                }
            }
        }
    }
}
