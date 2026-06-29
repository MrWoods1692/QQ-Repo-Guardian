# QQ Repo Guardian

QQ Repo Guardian 是一个 Rust 编写的 GitHub 仓库通知机器人服务。它接收 GitHub Webhook，把 Issue、PR、Push、检查、贡献者、Release、Star、Fork 等变化按仓库配置发送到指定 QQ 群聊或私聊。

## 功能

- 多仓库配置，每个仓库可发送到多个群聊或私聊。
- 按功能开关通知：Issue、PR、Push、Checks、Contributors、Release、Star、Fork。
- GitHub Webhook HMAC SHA-256 签名校验。
- QQ 消息事件：收到 GitHub 链接时返回仓库卡片，被戳一戳回复“戳我干嘛”，被 @ 时 @ 回去。
- 管理员指令：`/repo-guardian ping`、`/repo-guardian repos`。
- QQ 发送适配器：`mock` 用于本地开发测试，`one_bot` 用于连接 NapCat、Lagrange.OneBot、LLOneBot 等 OneBot v11 HTTP API。

## 快速开始

```bash
cp config.example.toml config.toml
cargo run -- --config config.toml
```

服务默认监听 `127.0.0.1:8080`：

- `GET /health` 健康检查。
- `POST /github/webhook` GitHub Webhook 入口。
- `POST /qq/event` OneBot 事件入口。
- `GET /github/card?url=https://github.com/owner/repo` GitHub 链接解析测试。

## QQ 登录

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