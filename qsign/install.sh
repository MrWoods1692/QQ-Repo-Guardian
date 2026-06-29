#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
JAR_PATH="${QRG_QSIGN_JAR:-$ROOT_DIR/unidbg-fetch-qsign.jar}"
DOWNLOAD_URL="${QRG_QSIGN_DOWNLOAD_URL:-}"
DOWNLOAD_URLS="${QRG_QSIGN_DOWNLOAD_URLS:-}"
SHA256="${QRG_QSIGN_SHA256:-}"

if [ -f "$JAR_PATH" ]; then
    exit 0
fi

if [ -z "$DOWNLOAD_URL" ] && [ -f "$ROOT_DIR/source.env" ]; then
    # shellcheck disable=SC1091
    . "$ROOT_DIR/source.env"
    DOWNLOAD_URL="${QRG_QSIGN_DOWNLOAD_URL:-}"
    DOWNLOAD_URLS="${QRG_QSIGN_DOWNLOAD_URLS:-$DOWNLOAD_URLS}"
    SHA256="${QRG_QSIGN_SHA256:-$SHA256}"
fi

DEFAULT_DOWNLOAD_URLS="https://api.github.com/repos/CikeyQi/unidbg-fetch-qsign-shell/releases/latest
https://github.com/CikeyQi/unidbg-fetch-qsign-shell/releases/latest/download/unidbg-fetch-qsign-all.jar
https://github.com/CikeyQi/unidbg-fetch-qsign-shell/releases/latest/download/unidbg-fetch-qsign-1.1.9.zip
https://api.github.com/repos/CikeyQi/unidbg-fetch-qsign-gui/releases/latest
https://github.com/CikeyQi/unidbg-fetch-qsign-gui/releases/latest/download/unidbg-fetch-qsign-gui-xiaoqian.zip
https://github.com/fuqiuluo/unidbg-fetch-qsign/releases/latest/download/unidbg-fetch-qsign.jar
https://github.com/fuqiuluo/unidbg-fetch-qsign/releases/latest/download/unidbg-fetch-qsign-android-watch.jar
https://api.github.com/repos/fuqiuluo/unidbg-fetch-qsign/releases/latest
https://api.github.com/repos/fuqiuluo/unidbg-fetch-qsign-next/releases/latest"

if [ -n "$DOWNLOAD_URL" ]; then
    CANDIDATE_URLS="$DOWNLOAD_URL"
elif [ -n "$DOWNLOAD_URLS" ]; then
    CANDIDATE_URLS="$DOWNLOAD_URLS"
else
    CANDIDATE_URLS="$DEFAULT_DOWNLOAD_URLS"
fi

mkdir -p "$(dirname -- "$JAR_PATH")" "$ROOT_DIR/txlib" "$ROOT_DIR/logs"
TMP_PATH="$JAR_PATH.tmp.$$"
EXTRACT_DIR="$ROOT_DIR/.qsign-extract.$$"

cleanup() {
    rm -f "$TMP_PATH"
    rm -rf "$EXTRACT_DIR"
}
trap cleanup EXIT INT TERM

download_to() {
    url="$1"
    output="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fL --retry 2 --connect-timeout 15 --output "$output" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -O "$output" "$url"
    else
        return 127
    fi
}

resolve_release_asset_url() {
    api_url="$1"
    json_path="$ROOT_DIR/logs/qsign-release.$$.json"
    if ! download_to "$api_url" "$json_path" >/dev/null 2>&1; then
        rm -f "$json_path"
        return 1
    fi
    asset_url="$(sed -n 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\.jar\)".*/\1/p' "$json_path" | head -n 1)"
    if [ -z "$asset_url" ]; then
        asset_url="$(sed -n 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\.zip\)".*/\1/p' "$json_path" | head -n 1)"
    fi
    rm -f "$json_path"
    [ -n "$asset_url" ] || return 1
    printf '%s\n' "$asset_url"
}

extract_jar_from_zip() {
    zip_path="$1"
    mkdir -p "$EXTRACT_DIR"
    if command -v unzip >/dev/null 2>&1; then
        unzip -q "$zip_path" -d "$EXTRACT_DIR"
    elif command -v jar >/dev/null 2>&1; then
        (cd "$EXTRACT_DIR" && jar xf "$zip_path")
    else
        echo "需要 unzip 或 jar 才能从 qsign zip 中提取 jar。" >&2
        return 127
    fi
    find "$EXTRACT_DIR" -type f -name '*.jar' | sort | head -n 1
}

if ! command -v curl >/dev/null 2>&1 && ! command -v wget >/dev/null 2>&1; then
    echo "需要 curl 或 wget 才能自动下载 qsign。" >&2
    exit 127
fi

DOWNLOAD_SUCCEEDED=0
printf '%s\n' "$CANDIDATE_URLS" | while IFS= read -r candidate_url; do
    [ -n "$candidate_url" ] || continue
    case "$candidate_url" in
        */releases/latest)
            asset_url="$(resolve_release_asset_url "$candidate_url" || true)"
            [ -n "$asset_url" ] || {
                echo "跳过 qsign release API: $candidate_url" >&2
                continue
            }
            candidate_url="$asset_url"
            ;;
    esac

    echo "正在安装 qsign: $candidate_url" >&2
    if download_to "$candidate_url" "$TMP_PATH"; then
        DOWNLOAD_SUCCEEDED=1
        break
    fi
    rm -f "$TMP_PATH"
done

if [ ! -s "$TMP_PATH" ]; then
    cat >&2 <<EOF
qsign 自动安装失败：内置下载地址均不可用。

可以稍后重试，或把 qsign jar 放到：$JAR_PATH
也可以用 QRG_QSIGN_DOWNLOAD_URL 临时指定下载地址。
EOF
    exit 127
fi

case "$TMP_PATH" in
    *.zip) ;;
    *)
        if file "$TMP_PATH" 2>/dev/null | grep -qi 'zip archive'; then
            ZIP_JAR_PATH="$(extract_jar_from_zip "$TMP_PATH")"
            if [ -z "$ZIP_JAR_PATH" ]; then
                echo "qsign zip 中没有找到 jar 文件。" >&2
                exit 1
            fi
            cp "$ZIP_JAR_PATH" "$TMP_PATH.jar"
            mv "$TMP_PATH.jar" "$TMP_PATH"
        fi
        ;;
esac

if [ -n "$SHA256" ]; then
    ACTUAL_SHA256="$(sha256sum "$TMP_PATH" | awk '{print $1}')"
    if [ "$ACTUAL_SHA256" != "$SHA256" ]; then
        echo "qsign 下载文件 SHA256 不匹配。" >&2
        echo "期望: $SHA256" >&2
        echo "实际: $ACTUAL_SHA256" >&2
        exit 1
    fi
fi

mv "$TMP_PATH" "$JAR_PATH"
echo "qsign 已安装到 $JAR_PATH" >&2