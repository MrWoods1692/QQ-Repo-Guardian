# qsign sidecar

QQ Repo Guardian 会在 `cargo run` 时自动调用 `./qsign/start.sh` 启动 qsign sidecar，然后等待 `http://127.0.0.1:8081` 可达后继续 QQ 扫码登录。

如果 qsign 尚未安装，`start.sh` 会先调用 `install.sh`，自动从内置候选源下载安装到 `qsign/unidbg-fetch-qsign.jar`。

离线安装时，也可以直接把本机可用的 qsign 服务包放到这里：

- `qsign/unidbg-fetch-qsign.jar`
- 或者 `qsign/bin/unidbg-fetch-qsign`

也可以用环境变量覆盖：

- `QRG_QSIGN_JAR=/path/to/unidbg-fetch-qsign.jar`
- `QRG_QSIGN_DOWNLOAD_URL=https://.../unidbg-fetch-qsign.jar`
- `QRG_QSIGN_DOWNLOAD_URLS=https://.../a.jar`，多地址时每行一个
- `QRG_QSIGN_SHA256=<jar 的 sha256>`
- `QRG_QSIGN_PORT=8081`
- `QRG_QSIGN_KEY=114514`

本目录不会提交 jar、`source.env`、txlib 和运行日志。需要固定下载源或校验值时，可以复制 `qsign/source.env.example` 为 `qsign/source.env` 后填写覆盖项。