# QQ Repo Guardian

QQ Repo Guardian 是一个 Rust 编写的 GitHub 仓库通知机器人服务。它会解析 GitHub 仓库页面获取提交变化，也可以接收 GitHub Webhook，把 Issue、PR、Push、检查、贡献者、Release、Star、Fork 等变化按仓库配置发送到指定 QQ 群聊或私聊。

## 功能

- 多仓库配置，每个仓库可发送到多个群聊或私聊。
- 通过 GitHub 公开页面定时检查仓库提交变化，配置里不需要 GitHub Token。
- 可选 GitHub Webhook：支持 Issue、PR、Push、Checks、Contributors、Release、Star、Fork，并支持 HMAC SHA-256 签名校验。
- QQ 消息事件：收到 GitHub 链接时返回仓库卡片，被戳一戳回复“戳我干嘛”，被 @ 时 @ 回去。
- 管理员指令：`/repo-guardian ping`、`/repo-guardian repos`。
- QQ 核心：使用 NapCat HTTP API，不使用 qsign。

## 快速开始

```bash
cp config.example.toml config.toml
cargo run
```

默认配置会自动下载并启动 NapCat Shell，不使用 qsign。首次运行时只需要在终端按 NapCat 提示扫码登录；扫码成功后再次启动会优先使用 NapCat 快速登录。如果本机没有安装 Linux QQ，请先安装 QQ，或设置 `QRG_QQ_BIN=/path/to/qq` 指向 QQ 可执行文件。

服务默认监听 `127.0.0.1:8080`：

- `GET /health` 健康检查。
- `POST /github/webhook` GitHub Webhook 入口。
- `POST /qq/event` QQ 消息事件入口。
- `GET /github/card?url=https://github.com/owner/repo` GitHub 仓库卡片 HTML 预览。
- `GET /github/card.svg?url=https://github.com/owner/repo` GitHub 仓库卡片图片。
- `GET /github/card.png?url=https://github.com/owner/repo` GitHub 仓库卡片 PNG 图片，QQ 消息默认使用这个地址。
- `GET /github/change.svg?...` GitHub 仓库变化通知 SVG 图片。
- `GET /github/change.png?...` GitHub 仓库变化通知 PNG 图片，Push webhook 和网页轮询推送默认使用这个地址。

## NapCat 配置

### HTTP API

默认 `cargo run` 会自动调用 `./napcat/start.sh`，下载 NapCat Shell、生成 OneBot HTTP 配置、启动 NapCat，并等待 NapCat HTTP API 可用。首次运行只需要在终端扫描 NapCat 显示的二维码；后续启动会自动尝试读取本机 Linux QQ 登录记录并传给 NapCat 的 `-q` 快速登录参数。Linux QQ 自动更新后，脚本也会优先读取 `~/.config/QQ/versions/config.json`，让 NapCat 使用当前实际运行的 QQ 版本文件。

如果自动识别不到账号，可以手动指定：

```bash
QRG_NAPCAT_QQ=123456789 cargo run
```

脚本会自动生成 NapCat 的事件上报地址：

```text
http://127.0.0.1:8080/qq/event
```

`config.toml` 可以显式写 NapCat 地址：

```toml
[bot]
type = "napcat"
endpoint = "http://127.0.0.1:3000"
command = "./napcat/start.sh"
timeout_secs = 180
# token = "你的 NapCat access token"
```

也可以用环境变量临时覆盖：`QRG_NAPCAT_ENDPOINT`、`QRG_NAPCAT_TOKEN`、`QRG_NAPCAT_COMMAND`、`QRG_NAPCAT_VERSION`、`QRG_NAPCAT_DOWNLOAD_URL`、`QRG_NAPCAT_DIR`、`QRG_NAPCAT_QQ`、`QRG_QQ_BIN`、`QRG_QQ_CONFIG_DIR`、`QRG_SERVER_URL`。发送群消息会调用 NapCat 的 `send_group_msg`，发送私聊会调用 `send_private_msg`。

收到 GitHub 仓库链接时，机器人会发送彩色仓库卡片图片。卡片包含仓库名、作者、头像、链接、Star、Fork、Issue、最近提交时间和 About。`QRG_SERVER_URL` 需要设置成 NapCat 能访问到的服务地址；默认是当前监听地址。

仓库变化推送也会发送彩色图片卡片，并保留文字详情作为降级内容。Push webhook 会展示分支、推送者、提交数量、最近提交摘要和 compare 链接；网页轮询会展示最新提交、作者和提交链接。

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

程序默认每 300 秒解析一次 `https://github.com/<GitHub名称>/<仓库名称>/commits.atom`，检测到新提交后发送 Push 类通知。首次启动会先记录当前最新提交，不会把历史提交刷屏。可以用 `[poller]` 调整轮询速度和 GitHub 请求超时：

```toml
[poller]
enabled = true
interval_secs = 60
timeout_secs = 10
```

`interval_secs` 最小按 30 秒执行；`timeout_secs` 默认 15 秒，最小按 3 秒执行。如果日志里出现 `GitHub page poll failed` 且包含 `Connection timed out`，说明当前机器访问 GitHub 超时，程序会等待下一轮继续重试。

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