# QQ Repo Guardian

QQ Repo Guardian 是一个 Rust 编写的 GitHub 仓库通知机器人服务。它接收 GitHub Webhook，把 Issue、PR、Push、检查、贡献者、Release、Star、Fork 等变化按仓库配置发送到指定 QQ 群聊或私聊。

## 功能

- 多仓库配置，每个仓库可发送到多个群聊或私聊。
- 按功能开关通知：Issue、PR、Push、Checks、Contributors、Release、Star、Fork。
- GitHub Webhook HMAC SHA-256 签名校验。
- QQ 消息事件：收到 GitHub 链接时返回仓库卡片，被戳一戳回复“戳我干嘛”，被 @ 时 @ 回去。
- 管理员指令：`/repo-guardian ping`、`/repo-guardian repos`。
- QQ 发送适配器：`mock` 用于本地开发测试，`one_bot` 用于连接 NapCat、Lagrange.OneBot、LLOneBot 等 OneBot v11 HTTP API，`proc_qq` 用于内置扫码登录。

## 快速开始

```bash
cp config.example.toml config.toml
cargo run
```

默认配置使用内置 QQ 扫码登录。程序会自动使用本地 qsign 默认配置：`http://127.0.0.1:8081` 和 key `114514`，并自动调用 `./qsign/start.sh` 安装和启动 qsign sidecar。如果没有 `session.token`，启动后会在终端打印二维码，扫码确认后保存登录态。

服务默认监听 `127.0.0.1:8080`：

- `GET /health` 健康检查。
- `POST /github/webhook` GitHub Webhook 入口。
- `POST /qq/event` OneBot 事件入口。
- `GET /github/card?url=https://github.com/owner/repo` GitHub 链接解析测试。

## QQ 登录

### 内置扫码登录

默认 `cargo run` 就会使用内置 QQ 客户端扫码登录。`config.toml` 中的 bot 配置如下：

```toml
[bot]
type = "proc_qq"
device_path = "device.json"
session_path = "session.token"
qsign_endpoint = "http://127.0.0.1:8081"
qsign_key = "114514"
qsign_command = "./qsign/start.sh"
qsign_timeout_secs = 60
```

项目已固定到上游 Git 版 `proc_qq` 依赖，并 patch 到本仓库的 `vendor/ricq`，首次构建需要能访问 GitHub 拉取依赖：

```bash
cargo run -- --config config.toml
```

启动时会先删除旧的 `qrcode.png`，首次登录会把二维码打印到终端，用手机 QQ 扫码确认后会把设备信息保存到 `device.json`，把登录态保存到 `session.token`。后续启动会优先尝试使用 `session.token` 自动登录。

默认 `qsign_command` 指向本仓库的 `qsign/start.sh`。脚本会优先使用 `qsign/unidbg-fetch-qsign.jar`，也支持 `qsign/bin/unidbg-fetch-qsign` 或通过 `QRG_QSIGN_JAR` 指向 jar 路径。如果本地没有 qsign jar，脚本会调用 `qsign/install.sh` 从内置候选源自动安装。也可以用环境变量临时覆盖：`QRG_QSIGN_ENDPOINT`、`QRG_QSIGN_KEY`、`QRG_QSIGN_COMMAND`、`QRG_QSIGN_PORT`、`QRG_QSIGN_DOWNLOAD_URL`、`QRG_QSIGN_DOWNLOAD_URLS`。如果 20 秒后登录仍未完成且终端没有二维码，先检查当前网络是否能连通 QQ 登录服务器；如果二维码已出现但日志反复出现 `failed to sign packet` 或 `Connection refused`，说明 `qsign_endpoint` 对应的 `/sign` 接口不可用；如果出现设备锁或滑块验证，请按终端日志里的地址完成验证。

如果只想本地开发 HTTP 和 Webhook，不启动 QQ 扫码，可以把 `[bot]` 改为 `type = "mock"`，并使用 `cargo run --no-default-features`。

### OneBot 登录

本项目生产模式推荐通过 OneBot 适配器接入 QQ。NapCat、Lagrange.OneBot 或 LLOneBot 负责弹出二维码、扫码登录、保存登录状态和自动重连；本服务负责接收事件并发送消息。

在 `config.toml` 中把 bot 改成：

```toml
[bot]
type = "one_bot"
endpoint = "http://127.0.0.1:3000"
access_token = "optional-token"
```

然后把 OneBot HTTP 事件上报地址配置为：

```text
http://127.0.0.1:8080/qq/event
```

## GitHub Webhook

在仓库或组织设置中添加 Webhook：

- Payload URL: `http://你的服务地址/github/webhook`
- Content type: `application/json`
- Secret: 与 `github.webhook_secret` 保持一致
- Events: Issues、Pull requests、Pushes、Check runs、Check suites、Members、Releases、Stars、Forks

## 测试

```bash
cargo test
```