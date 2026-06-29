# qsign sidecar

QQ Repo Guardian 会在 `cargo run` 时自动调用 `./qsign/start.sh` 启动 qsign sidecar，然后等待 `http://127.0.0.1:8081` 可达后继续 QQ 扫码登录。

如果 qsign 尚未安装，`start.sh` 会先调用 `install.sh`，自动从内置候选源下载安装到本目录。优先使用完整 qsign zip 包，因为 qsign 启动时不仅需要服务本体，还需要 `txlib/<版本>/` 里的资源文件。

离线安装时，也可以直接把本机可用的 qsign 服务本体放到这里：

- `qsign/unidbg-fetch-qsign.jar`
- 或者 `qsign/bin/unidbg-fetch-qsign`

同时需要准备一个完整的 `txlib` 版本目录，例如：

- `qsign/txlib/8.9.76/libfekit.so`
- `qsign/txlib/8.9.76/config.json`
- `qsign/txlib/8.9.76/dtconfig.json`

启动参数 `--basePath` 会指向具体版本目录，也就是 `qsign/txlib/8.9.76`，不是 `qsign/txlib` 根目录。
没有显式设置 `QRG_QSIGN_BASE_PATH` 时，`start.sh` 默认优先选择 `8.9.85`，这个版本通常比最新 `txlib` 更稳定；如果本地没有该版本，再自动选择最高版本的可用 `txlib`。

也可以用环境变量覆盖：

- `QRG_QSIGN_JAR=/path/to/unidbg-fetch-qsign.jar`
- `QRG_QSIGN_BIN=/path/to/unidbg-fetch-qsign`
- `QRG_QSIGN_BASE_PATH=/path/to/txlib/version`，也可以指向包含版本子目录的 `txlib` 根目录
- `QRG_QSIGN_PREFERRED_VERSION=8.9.85`，在 `QRG_QSIGN_BASE_PATH` 指向 `txlib` 根目录时指定优先版本
- `QRG_QSIGN_DOWNLOAD_URL=https://.../unidbg-fetch-qsign.jar`
- `QRG_QSIGN_DOWNLOAD_URLS=https://.../a.zip`，多地址时每行一个
- `QRG_QSIGN_SHA256=<下载文件的 sha256>`
- `QRG_QSIGN_PORT=8081`
- `QRG_QSIGN_KEY=114514`

本目录不会提交 jar、`source.env`、txlib 和运行日志。需要固定下载源或校验值时，可以复制 `qsign/source.env.example` 为 `qsign/source.env` 后填写覆盖项。