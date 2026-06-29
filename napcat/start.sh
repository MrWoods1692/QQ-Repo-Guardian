#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
NAPCAT_DIR="${QRG_NAPCAT_DIR:-$ROOT_DIR/napcat}"
NAPCAT_VERSION="${QRG_NAPCAT_VERSION:-v4.18.7}"
NAPCAT_ZIP="${QRG_NAPCAT_ZIP:-$NAPCAT_DIR/NapCat.Shell.zip}"
NAPCAT_URL="${QRG_NAPCAT_DOWNLOAD_URL:-https://github.com/NapNeko/NapCatQQ/releases/download/$NAPCAT_VERSION/NapCat.Shell.zip}"
LAUNCHER_CPP_URL="${QRG_NAPCAT_LAUNCHER_CPP_URL:-https://raw.githubusercontent.com/NapNeko/napcat-linux-launcher/refs/heads/main/launcher.cpp}"
NAPCAT_ENDPOINT="${QRG_NAPCAT_ENDPOINT:-http://127.0.0.1:3000}"
NAPCAT_TOKEN="${QRG_NAPCAT_TOKEN:-}"
QRG_SERVER_URL="${QRG_SERVER_URL:-http://127.0.0.1:8080}"
DISPLAY_NUMBER="${QRG_NAPCAT_DISPLAY:-:1}"
START_XVFB="${QRG_NAPCAT_XVFB:-auto}"
QQ_BIN="${QRG_QQ_BIN:-}"
NAPCAT_QQ="${QRG_NAPCAT_QQ:-${QRG_QQ_ACCOUNT:-}}"
QQ_CONFIG_DIR="${QRG_QQ_CONFIG_DIR:-$HOME/.config/QQ}"
DRY_RUN="${QRG_NAPCAT_DRY_RUN:-0}"

read_qq_current_version() {
  config_path="$QQ_CONFIG_DIR/versions/config.json"
  if [ ! -f "$config_path" ]; then
    return 0
  fi

  sed -n 's/.*"curVersion"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$config_path" | head -n 1
}

configure_qq_version_paths() {
  current_version=$(read_qq_current_version)
  if [ -z "$current_version" ]; then
    return 0
  fi

  version_dir="$QQ_CONFIG_DIR/versions/$current_version"
  if [ ! -d "$version_dir" ]; then
    return 0
  fi

  if [ -f "$version_dir/package.json" ]; then
    export NAPCAT_QQ_PACKAGE_INFO_PATH="${NAPCAT_QQ_PACKAGE_INFO_PATH:-$version_dir/package.json}"
  fi
  if [ -f "$QQ_CONFIG_DIR/versions/config.json" ]; then
    export NAPCAT_QQ_VERSION_CONFIG_PATH="${NAPCAT_QQ_VERSION_CONFIG_PATH:-$QQ_CONFIG_DIR/versions/config.json}"
  fi
  if [ -f "$version_dir/wrapper.node" ]; then
    export NAPCAT_WRAPPER_PATH="${NAPCAT_WRAPPER_PATH:-$version_dir/wrapper.node}"
  fi

  echo "Detected Linux QQ quick-update version $current_version."
}

detect_quick_login_qq() {
  if ! command -v strings >/dev/null 2>&1; then
    return 0
  fi

  for db in \
    "$HOME/.config/QQ/global/nt_db/login.db" \
    "$HOME/.config/QQ/nt_qq/global/nt_db/login.db"
  do
    if [ -f "$db" ]; then
      strings "$db" 2>/dev/null | sed -n 's/.*\([1-9][0-9]\{4,11\}\).*/\1/p' | head -n 1
      return 0
    fi
  done
}

if [ -z "$QQ_BIN" ]; then
  for candidate in /opt/QQ/qq /usr/bin/qq /usr/local/bin/qq; do
    if [ -x "$candidate" ]; then
      QQ_BIN="$candidate"
      break
    fi
  done
fi

if [ -z "$QQ_BIN" ] || [ ! -x "$QQ_BIN" ]; then
  echo "Linux QQ executable was not found." >&2
  echo "Install Linux QQ first, or set QRG_QQ_BIN=/path/to/qq." >&2
  echo "NapCat release notes recommend QQ 3.2.23 / build 44343 or newer." >&2
  exit 1
fi

if [ -z "$NAPCAT_QQ" ]; then
  NAPCAT_QQ=$(detect_quick_login_qq)
fi

configure_qq_version_paths

if [ "$DRY_RUN" = "1" ]; then
  echo "NapCat dry run"
  echo "QQ_BIN=$QQ_BIN"
  echo "NAPCAT_QQ=$NAPCAT_QQ"
  echo "NAPCAT_QQ_PACKAGE_INFO_PATH=${NAPCAT_QQ_PACKAGE_INFO_PATH:-}"
  echo "NAPCAT_QQ_VERSION_CONFIG_PATH=${NAPCAT_QQ_VERSION_CONFIG_PATH:-}"
  echo "NAPCAT_WRAPPER_PATH=${NAPCAT_WRAPPER_PATH:-}"
  exit 0
fi

mkdir -p "$NAPCAT_DIR"

if [ ! -f "$NAPCAT_ZIP" ]; then
  echo "Downloading NapCat Shell $NAPCAT_VERSION..."
  if command -v curl >/dev/null 2>&1; then
    curl -L --fail --connect-timeout 15 --max-time 600 -o "$NAPCAT_ZIP" "$NAPCAT_URL"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$NAPCAT_ZIP" "$NAPCAT_URL"
  else
    echo "curl or wget is required to download NapCat" >&2
    exit 1
  fi
fi

if [ ! -f "$NAPCAT_DIR/napcat.mjs" ] || [ ! -f "$NAPCAT_DIR/loadNapCat.js" ]; then
  echo "Extracting NapCat Shell..."
  unzip -oq "$NAPCAT_ZIP" -d "$NAPCAT_DIR"
fi

if [ ! -f "$NAPCAT_DIR/libnapcat_launcher.so" ]; then
  if ! command -v g++ >/dev/null 2>&1; then
    echo "g++ is required to build NapCat Linux launcher." >&2
    echo "Install g++, or place libnapcat_launcher.so in $NAPCAT_DIR." >&2
    exit 1
  fi

  echo "Downloading NapCat Linux launcher..."
  if command -v curl >/dev/null 2>&1; then
    curl -L --fail --connect-timeout 15 --max-time 120 -o "$NAPCAT_DIR/launcher.cpp" "$LAUNCHER_CPP_URL"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$NAPCAT_DIR/launcher.cpp" "$LAUNCHER_CPP_URL"
  else
    echo "curl or wget is required to download NapCat Linux launcher" >&2
    exit 1
  fi

  echo "Building NapCat Linux launcher..."
  g++ -shared -fPIC "$NAPCAT_DIR/launcher.cpp" -o "$NAPCAT_DIR/libnapcat_launcher.so" -ldl
fi

PORT=$(printf '%s\n' "$NAPCAT_ENDPOINT" | sed -n 's#.*://[^:/]*:\([0-9][0-9]*\).*#\1#p')
if [ -z "$PORT" ]; then
  PORT=3000
fi

mkdir -p "$NAPCAT_DIR/config"
cat > "$NAPCAT_DIR/config/onebot11.json" <<EOF
{
  "network": {
    "httpServers": [
      {
        "name": "qq-repo-guardian-http",
        "enable": true,
        "port": $PORT,
        "host": "127.0.0.1",
        "enableCors": true,
        "enableWebsocket": true,
        "messagePostFormat": "string",
        "token": "$NAPCAT_TOKEN",
        "debug": false
      }
    ],
    "httpClients": [
      {
        "name": "qq-repo-guardian-event",
        "enable": true,
        "url": "$QRG_SERVER_URL/qq/event",
        "messagePostFormat": "string",
        "reportSelfMessage": false,
        "token": "$NAPCAT_TOKEN",
        "debug": false
      }
    ],
    "websocketServers": [],
    "websocketClients": []
  },
  "musicSignUrl": "",
  "enableLocalFile2Url": false,
  "parseMultMsg": false
}
EOF

cd "$ROOT_DIR"

if [ "$START_XVFB" = "1" ] || { [ "$START_XVFB" = "auto" ] && command -v Xvfb >/dev/null 2>&1 && [ -z "${DISPLAY:-}" ]; }; then
  Xvfb "$DISPLAY_NUMBER" -screen 0 1x1x8 +extension GLX +render >/dev/null 2>&1 &
  export DISPLAY="$DISPLAY_NUMBER"
elif [ -z "${DISPLAY:-}" ]; then
  echo "DISPLAY is not set and Xvfb was not started." >&2
  echo "Install xvfb, set DISPLAY, or set QRG_NAPCAT_XVFB=0 if your environment already handles display." >&2
  exit 1
fi

echo "Starting NapCat with $QQ_BIN"
if [ -n "$NAPCAT_QQ" ]; then
  echo "Using NapCat quick login account $NAPCAT_QQ."
  echo "If quick login fails, scan the QR code shown by NapCat in this terminal."
  exec env NAPCAT_BOOTMAIN="$ROOT_DIR" LD_PRELOAD="$NAPCAT_DIR/libnapcat_launcher.so" "$QQ_BIN" --no-sandbox -q "$NAPCAT_QQ" "$@"
fi

echo "If this is the first run, scan the QR code shown by NapCat in this terminal."
echo "After a successful scan, set QRG_NAPCAT_QQ=<your QQ number> if automatic quick login detection misses it."
exec env NAPCAT_BOOTMAIN="$ROOT_DIR" LD_PRELOAD="$NAPCAT_DIR/libnapcat_launcher.so" "$QQ_BIN" --no-sandbox "$@"