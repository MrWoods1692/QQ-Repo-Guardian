#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
PORT="${QRG_QSIGN_PORT:-8081}"
KEY="${QRG_QSIGN_KEY:-114514}"
BASE_PATH="${QRG_QSIGN_BASE_PATH:-$ROOT_DIR/txlib}"
JAR_PATH="${QRG_QSIGN_JAR:-$ROOT_DIR/unidbg-fetch-qsign.jar}"
BIN_PATH="${QRG_QSIGN_BIN:-$ROOT_DIR/bin/unidbg-fetch-qsign}"
RUNTIME_BASE_PATH="$ROOT_DIR/.runtime/txlib"

bin_path_is_ready() {
  [ -x "$BIN_PATH" ] || return 1
  [ -d "$ROOT_DIR/lib" ] || return 1
  find "$ROOT_DIR/lib" -type f -name '*.jar' | grep -q .
}

jar_path_is_ready() {
  [ -f "$JAR_PATH" ] || return 1
  unzip -p "$JAR_PATH" META-INF/MANIFEST.MF 2>/dev/null | grep -q '^Main-Class: MainKt'
}

base_path_is_ready() {
  base_path="$1"
  [ -d "$base_path" ] || return 1
  [ -f "$base_path/libfekit.so" ] || return 1
  [ -f "$base_path/config.json" ] || return 1
  [ -f "$base_path/dtconfig.json" ] || return 1
}

select_base_path() {
  if base_path_is_ready "$BASE_PATH"; then
    printf '%s\n' "$BASE_PATH"
    return 0
  fi

  for candidate_path in "$BASE_PATH"/* "$ROOT_DIR"/txlib/*; do
    if base_path_is_ready "$candidate_path"; then
      printf '%s\n' "$candidate_path"
      return 0
    fi
  done

  return 1
}

prepare_base_path() {
  source_path="$1"
  rm -rf "$RUNTIME_BASE_PATH"
  mkdir -p "$(dirname "$RUNTIME_BASE_PATH")"
  cp -R "$source_path" "$RUNTIME_BASE_PATH"
  sed -i \
    -e 's/"port"[[:space:]]*:[[:space:]]*[0-9][0-9]*/"port": '"$PORT"'/' \
    -e 's/"key"[[:space:]]*:[[:space:]]*"[^"]*"/"key": "'"$KEY"'"/' \
    "$RUNTIME_BASE_PATH/config.json"
  printf '%s\n' "$RUNTIME_BASE_PATH"
}

if { ! bin_path_is_ready && ! jar_path_is_ready; } || ! select_base_path >/dev/null; then
  "$ROOT_DIR/install.sh"
fi

if ! BASE_PATH="$(select_base_path)"; then
  cat >&2 <<EOF
qsign txlib 不完整，缺少 libfekit.so、config.json 或 dtconfig.json。

请安装完整 qsign 包，或设置：
  QRG_QSIGN_BASE_PATH=/path/to/txlib/version

然后重新运行 cargo run。
EOF
  exit 127
fi

BASE_PATH="$(prepare_base_path "$BASE_PATH")"

if bin_path_is_ready; then
    exec "$BIN_PATH" --basePath="$BASE_PATH" --port="$PORT" --key="$KEY"
fi

if jar_path_is_ready; then
  exec java -jar "$JAR_PATH" --basePath="$BASE_PATH" --port="$PORT" --key="$KEY"
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