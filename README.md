# QQ Repo Guardian

QQ Repo Guardian 是一个 Rust 编写的 GitHub 仓库通知机器人服务。它会解析 GitHub 仓库页面获取提交变化，也可以接收 GitHub Webhook，把 Issue、PR、Push、检查、贡献者、Release、Star、Fork 等变化按仓库配置发送到指定 QQ 群聊或私聊。

## 功能

- 多仓库配置，每个仓库可发送到多个群聊或私聊。
- 通过 GitHub 公开页面定时检查仓库提交变化，配置里不需要 GitHub Token。
- 可选 GitHub Webhook：支持 Issue、PR、Push、Checks、Contributors、Release、Star、Fork，并支持 HMAC SHA-256 签名校验。
- QQ 消息事件：收到 GitHub 链接时返回仓库卡片，被戳一戳回复“戳我干嘛”，被 @ 时 @ 回去。
- 管理员指令：`/repo-guardian ping`、`/repo-guardian repos`。
- QQ 登录：只使用内置 QQ 扫码登录。

## 快速开始

```bash
cp config.example.toml config.toml
cargo run
```

默认配置使用内置 QQ 扫码登录。程序会自动使用本地 qsign 默认配置：`http://127.0.0.1:8081` 和 key `114514`，并自动调用 `./qsign/start.sh` 安装和启动 qsign sidecar。如果没有 `session.token`，启动后会在终端打印二维码，扫码确认后保存登录态。

服务默认监听 `127.0.0.1:8080`：

- `GET /health` 健康检查。
- `POST /github/webhook` GitHub Webhook 入口。
- `POST /qq/event` QQ 消息事件入口。
- `GET /github/card?url=https://github.com/owner/repo` GitHub 链接解析测试。

## QQ 登录

### 内置扫码登录

默认 `cargo run` 就会使用内置 QQ 客户端扫码登录，不需要在 `config.toml` 里填写 QQ 登录参数。

项目已固定到上游 Git 版 `proc_qq` 依赖，并 patch 到本仓库的 `vendor/ricq`，首次构建需要能访问 GitHub 拉取依赖：

```bash
cargo run -- --config config.toml
```

启动时会先删除旧的 `qrcode.png`，首次登录会把二维码打印到终端，用手机 QQ 扫码确认后会把设备信息保存到 `device.json`，把登录态保存到 `session.token`。后续启动会优先尝试使用 `session.token` 自动登录。

默认 `qsign_command` 指向本仓库的 `qsign/start.sh`。脚本会优先使用完整 qsign zip，安装服务本体和 `txlib/<版本>` 资源；也支持手动放置 `qsign/bin/unidbg-fetch-qsign` 或 `qsign/unidbg-fetch-qsign.jar`。首次下载完整包可能较慢，所以默认 `qsign_timeout_secs = 900`。也可以用环境变量临时覆盖：`QRG_QSIGN_ENDPOINT`、`QRG_QSIGN_KEY`、`QRG_QSIGN_COMMAND`、`QRG_QSIGN_PORT`、`QRG_QSIGN_DOWNLOAD_URL`、`QRG_QSIGN_DOWNLOAD_URLS`、`QRG_QSIGN_BASE_PATH`。如果 20 秒后登录仍未完成且终端没有二维码，先检查当前网络是否能连通 QQ 登录服务器；如果二维码已出现但日志反复出现 `failed to sign packet` 或 `Connection refused`，说明 `qsign_endpoint` 对应的 `/sign` 接口不可用；如果出现设备锁或滑块验证，请按终端日志里的地址完成验证。

## 仓库通知配置

`config.toml` 只需要填写管理员 QQ、GitHub 名称、仓库名称，以及接收消息的 QQ 群或私聊：

```toml
admins = [123456789]

[[repositories]]
github = "owner"
repo = "repo"
groups = [10000]
privates = [123456789]
```

继续添加仓库时，再复制一组 `[[repositories]]`。`groups` 和 `privates` 都支持多个 QQ 号；没有对应接收方时填空数组 `[]`。

程序默认每 300 秒解析一次 `https://github.com/<GitHub名称>/<仓库名称>/commits.atom`，检测到新提交后发送 Push 类通知。首次启动会先记录当前最新提交，不会把历史提交刷屏。

## 可选 GitHub Webhook

网页轮询主要用于提交变化。如果还需要 Issue、PR、Checks、Contributors、Release、Star、Fork 等事件，可以继续配置 GitHub Webhook。

在仓库或组织设置中添加 Webhook：

- Payload URL: `http://你的服务地址/github/webhook`
- Content type: `application/json`
- Secret: 可选；如需启用，在 `config.toml` 中添加 `[github] webhook_secret = "你的密钥"`
- Events: Issues、Pull requests、Pushes、Check runs、Check suites、Members、Releases、Stars、Forks

高级开关仍支持旧格式 `[github.default_features]`、`[[github.repositories]]`、`[github.repositories.features]` 和 `[[github.repositories.targets]]`，不需要时可以忽略。

## 测试

```bash
cargo test
```