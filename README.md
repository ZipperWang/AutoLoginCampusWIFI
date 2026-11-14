# AutoLoginCampusWIFI

西北农林科技大学校园网命令行客户端。支持 Windows、Linux 和 macOS，通过 HTTPS 与学校深澜门户通信。Linux 可在纯终端中使用。

## 编译与运行

安装 Rust 稳定版后，在仓库根目录运行以下命令。

```sh
cargo build --locked --release
```

Windows 可执行文件为 `target\release\campus.exe`，Linux 和 macOS 为 `target/release/campus`。将可执行文件放入 PATH 后，可以直接运行以下命令。

| 命令 | 用途 |
| --- | --- |
| `campus status` | 显示访问门户所用的接口、本地 IP 和门户报告的认证状态。 |
| `campus login` | 登录当前校园网，按需提示输入学号和密码。 |
| `campus login -u 学号 --remember` | 在需要认证时输入密码，认证成功后保存到系统凭据库。 |
| `campus login -u 学号 --connect-wifi` | 主动连接开放无线网络 NWAFU，等待该接口取得通往门户的路由后认证。 |
| `campus watch -u 学号` | 在前台持续监听网卡地址变化，按需认证。按 Ctrl+C 退出。 |

`watch` 同样支持 `--connect-wifi` 和 `--remember`。默认复查间隔为 60 秒，可以用 `--interval 120` 调整。临时网络故障会逐步延长重试间隔，账号错误或门户拒绝认证时会退出并提示检查凭据。

## 网络和凭据

默认使用已连接的网络。客户端直接查询 `portal.nwafu.edu.cn` 的会话状态。门户明确返回离线，且当前连接具有校园网线索（学校门户的私网地址、通往门户的 NWAFU 接口或指向学校门户的认证跳转）时，才会提交登录。单次登录的请求使用同一个本地地址和门户地址。

Windows 使用凭据管理器，macOS 使用钥匙串，Linux 使用当前用户的 Secret Service。已有凭据时可用 `campus login -u 学号` 直接登录。更新密码时，在需要认证的交互会话中再次使用 `--remember`。已经在线时直接返回，跳过密码读取及写入。

无桌面 Linux 可以交互输入，也可以让密码管理器将一行密码通过管道送入 `campus login -u 学号 --password-stdin` 或 `campus watch -u 学号 --password-stdin`。旧版使用的 `Account` 是明文文件，新版首次使用时请重新输入凭据。

Linux 上主动连接 Wi-Fi 需要 NetworkManager 和 `nmcli`。使用有线校园网或已经连接校园 Wi-Fi 的机器可直接运行 `login`。macOS、Windows 读取无线网络信息或主动连接时可能需要在系统设置中允许定位或无线网络访问。

### 退出码

| 值 | 含义 |
| --- | --- |
| 0 | 已在线、登录成功或正常结束监听 |
| 1 | 本地运行错误 |
| 2 | 参数或输入无效 |
| 3 | `status` 查询确认当前连接尚未认证 |
| 4 | 校园网、门户或认证状态暂时不可用 |
| 5 | 凭据不可用、保存失败或门户拒绝认证 |
| 6 | 当前用户已有认证请求在执行 |

## 开发与发布

`campus-core` 实现门户协议，`campus-platform` 负责平台网络接口、系统凭据和进程锁，`campus-cli` 提供终端入口。可运行 `cargo test --locked --workspace` 执行协议向量和本地模拟门户测试。

GitHub Actions 在拉取请求和推送到 `master` 时测试并编译 Windows x64、Linux x64、macOS Intel 和 Apple Silicon。运行产物在 Actions 页面下载。推送 `V*` 或 `v*` 标签后，工作流会把压缩包及 `SHA256SUMS` 发布到对应的 GitHub Release。
