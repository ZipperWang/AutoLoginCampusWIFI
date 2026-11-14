use std::io::{self, IsTerminal, Write};

use campus_core::self_service::{Mac, SelfService, ServiceError, Target};
use campus_platform::{CredentialError, credential};
use clap::{Args, Subcommand, ValueEnum};
use zeroize::Zeroizing;

#[derive(Args)]
pub(super) struct ServiceArgs {
    #[arg(
        short,
        long,
        global = true,
        value_name = "学号",
        help = "使用指定账号与人工验证码登录自助页"
    )]
    username: Option<String>,
    #[arg(
        long,
        global = true,
        help = "从标准输入读取一行密码，验证码仍由本人输入"
    )]
    password_stdin: bool,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    #[command(about = "列出当前账号的在线记录、MAC和IP")]
    Devices,
    #[command(about = "按MAC下线整台设备，或只下线一条在线记录")]
    Disconnect {
        #[arg(
            long,
            conflicts_with = "record",
            required_unless_present = "record",
            value_name = "MAC"
        )]
        mac: Option<Mac>,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..), value_name = "记录ID")]
        record: Option<u64>,
    },
    #[command(about = "查看或设置账号的无感知认证")]
    Seamless {
        #[arg(value_enum, default_value = "status")]
        mode: Mode,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Status,
    On,
    Off,
}

impl ServiceArgs {
    fn error(error: ServiceError) -> (u8, String) {
        let code = match error {
            ServiceError::Session | ServiceError::Login => 5,
            ServiceError::Missing => 3,
            _ => 4,
        };
        (code, error.to_string())
    }
}

pub(super) async fn run(args: ServiceArgs) -> Result<u8, (u8, String)> {
    let service = SelfService::new().map_err(ServiceArgs::error)?;
    if let Some(username) = args.username {
        let username = username.trim();
        if username.is_empty() {
            return Err((2, "请输入有效学号".into()));
        }
        let form = service.login_form().await.map_err(ServiceArgs::error)?;
        let interactive = io::stdin().is_terminal();
        let password = if args.password_stdin {
            let mut value = Zeroizing::new(String::new());
            io::stdin()
                .read_line(&mut value)
                .map_err(|_| (2, "读取标准输入失败".to_owned()))?;
            if value.ends_with('\n') {
                value.pop();
                if value.ends_with('\r') {
                    value.pop();
                }
            }
            value
        } else {
            let account = username.to_owned();
            let saved =
                tokio::task::spawn_blocking(move || credential(&account)?.get_password()).await;
            match saved {
                Ok(Ok(value)) => Zeroizing::new(value),
                _ if interactive => Zeroizing::new(
                    rpassword::prompt_password("自助服务密码：")
                        .map_err(|_| (2, "读取密码失败".to_owned()))?,
                ),
                Ok(Err(CredentialError::NoEntry)) => {
                    return Err((5, "该账号未保存密码，请使用 --password-stdin".into()));
                }
                _ => return Err((5, "系统凭据库不可用，请使用 --password-stdin".into())),
            }
        };
        if password.is_empty() {
            return Err((2, "密码为空".into()));
        }
        let mut code = String::new();
        if let Some(bytes) = &form.captcha_png {
            let mut decoder =
                image::ImageReader::with_format(io::Cursor::new(bytes), image::ImageFormat::Png);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(1024);
            limits.max_image_height = Some(512);
            decoder.limits(limits);
            let img = decoder
                .decode()
                .map_err(|_| (4, "无法读取自助页验证码图片".to_owned()))?
                .thumbnail(100, 40)
                .to_luma8();
            for y in (0..img.height()).step_by(2) {
                let line: String = (0..img.width())
                    .map(|x| {
                        let upper = img.get_pixel(x, y)[0] < 195;
                        let lower = y + 1 < img.height() && img.get_pixel(x, y + 1)[0] < 195;
                        match (upper, lower) {
                            (true, true) => '█',
                            (true, false) => '▀',
                            (false, true) => '▄',
                            _ => ' ',
                        }
                    })
                    .collect();
                eprintln!("{line}");
            }
            eprint!("请输入图中的验证码：");
            io::stderr()
                .flush()
                .map_err(|_| (1, "无法显示验证码提示".to_owned()))?;
            io::stdin()
                .read_line(&mut code)
                .map_err(|_| (2, "读取验证码失败".to_owned()))?;
            if code.trim().is_empty() {
                return Err((2, "请输入本次会话的验证码".into()));
            }
        }
        service
            .sign_in(form, username, &password, code.trim())
            .await
            .map_err(ServiceArgs::error)?;
    } else {
        if args.password_stdin {
            return Err((2, "--password-stdin 需要同时指定 --username".into()));
        }
        service.sign_in_online().await.map_err(ServiceArgs::error)?;
    }

    match args.command {
        Action::Devices => {
            let mut devices = service.devices().await.map_err(ServiceArgs::error)?;
            devices.sort_by_key(|device| (device.mac, device.id));
            if devices.is_empty() {
                println!("当前账号没有在线记录");
                return Ok(0);
            }
            println!("记录ID\tMAC\tIP");
            for device in devices {
                let mac = device
                    .mac
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".into());
                let ip = device
                    .ip
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".into());
                println!("{}\t{mac}\t{ip}", device.id);
            }
        }
        Action::Disconnect { mac, record } => {
            let target = match (mac, record) {
                (Some(m), None) => Target::Mac(m),
                (None, Some(id)) => Target::Record(id),
                _ => return Err((2, "请指定一个 MAC 或一个记录 ID".into())),
            };
            let count = service
                .disconnect(target)
                .await
                .map_err(ServiceArgs::error)?;
            println!("已确认所选的 {count} 条在线记录下线");
        }
        Action::Seamless { mode } => {
            let desired = match mode {
                Mode::Status => None,
                Mode::On => Some(true),
                Mode::Off => Some(false),
            };
            let state = service
                .seamless(desired)
                .await
                .map_err(ServiceArgs::error)?;
            println!(
                "当前账号无感知认证：{}",
                if state { "已开启" } else { "已关闭" }
            );
        }
    }
    Ok(0)
}
