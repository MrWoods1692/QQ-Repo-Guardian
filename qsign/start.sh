#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
PORT="${QRG_QSIGN_PORT:-8081}"
KEY="${QRG_QSIGN_KEY:-114514}"
BASE_PATH="${QRG_QSIGN_BASE_PATH:-$ROOT_DIR/txlib}"
JAR_PATH="${QRG_QSIGN_JAR:-$ROOT_DIR/unidbg-fetch-qsign.jar}"
BIN_PATH="${QRG_QSIGN_BIN:-$ROOT_DIR/bin/unidbg-fetch-qsign}"

if [ ! -f "$JAR_PATH" ] && [ ! -x "$BIN_PATH" ]; then
  "$ROOT_DIR/install.sh"
fi

if [ -f "$JAR_PATH" ]; then
    exec java -jar "$JAR_PATH" --basePath="$BASE_PATH" --port="$PORT" --key="$KEY"
fi

if [ -x "$BIN_PATH" ]; then
    exec "$BIN_PATH" --basePath="$BASE_PATH" --port="$PORT" --key="$KEY"
fi

cat >&2 <<EOF
没有找到 qsign 服务本体，自动安装也没有完成。

请把可用的 qsign 包放到以下任一位置：
  $ROOT_DIR/unidbg-fetch-qsign.jar
  $BIN_PATH

也可以设置：
  QRG_QSIGN_JAR=/path/to/unidbg-fetch-qsign.jar
  QRG_QSIGN_BIN=/path/to/unidbg-fetch-qsign
  QRG_QSIGN_DOWNLOAD_URL=https://.../unidbg-fetch-qsign.jar

然后重新运行 cargo run。
EOF
exit 127