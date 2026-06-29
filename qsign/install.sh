#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
JAR_PATH="${QRG_QSIGN_JAR:-$ROOT_DIR/unidbg-fetch-qsign.jar}"
BASE_PATH="${QRG_QSIGN_BASE_PATH:-$ROOT_DIR/txlib}"
BIN_PATH="${QRG_QSIGN_BIN:-$ROOT_DIR/bin/unidbg-fetch-qsign}"
DOWNLOAD_URL="${QRG_QSIGN_DOWNLOAD_URL:-}"
DOWNLOAD_URLS="${QRG_QSIGN_DOWNLOAD_URLS:-}"
SHA256="${QRG_QSIGN_SHA256:-}"

base_path_is_ready() {
    base_path="$1"
    [ -d "$base_path" ] || return 1
    [ -f "$base_path/libfekit.so" ] || return 1
    [ -f "$base_path/config.json" ] || return 1
    [ -f "$base_path/dtconfig.json" ] || return 1
}

bin_path_is_ready() {
    [ -x "$BIN_PATH" ] || return 1
    [ -d "$ROOT_DIR/lib" ] || return 1
    find "$ROOT_DIR/lib" -type f -name '*.jar' | grep -q .
}

jar_path_is_ready() {
    [ -f "$JAR_PATH" ] || return 1
    unzip -p "$JAR_PATH" META-INF/MANIFEST.MF 2>/dev/null | grep -q '^Main-Class: MainKt'
}

has_ready_txlib() {
    if base_path_is_ready "$BASE_PATH"; then
        return 0
    fi

    for candidate_path in "$BASE_PATH"/* "$ROOT_DIR"/txlib/*; do
        if base_path_is_ready "$candidate_path"; then
            return 0
        fi
    done

    return 1
}

txlib_root() {
    if base_path_is_ready "$BASE_PATH"; then
        dirname -- "$BASE_PATH"
    else
        printf '%s\n' "$BASE_PATH"
    fi
}

if { bin_path_is_ready || jar_path_is_ready; } && has_ready_txlib; then
    exit 0
fi

if { bin_path_is_ready || jar_path_is_ready; } && ! has_ready_txlib; then
        cat >&2 <<EOF
已找到 qsign 服务本体，但缺少完整 txlib 资源。
将继续安装完整 qsign 包；如果自动下载较慢，可以手动准备 txlib/<版本>/，其中必须包含：
    libfekit.so
    config.json
    dtconfig.json
EOF
fi

if [ -z "$DOWNLOAD_URL" ] && [ -f "$ROOT_DIR/source.env" ]; then
    # shellcheck disable=SC1091
    . "$ROOT_DIR/source.env"
    DOWNLOAD_URL="${QRG_QSIGN_DOWNLOAD_URL:-}"
    DOWNLOAD_URLS="${QRG_QSIGN_DOWNLOAD_URLS:-$DOWNLOAD_URLS}"
    SHA256="${QRG_QSIGN_SHA256:-$SHA256}"
fi

DEFAULT_DOWNLOAD_URLS="https://api.github.com/repos/CikeyQi/unidbg-fetch-qsign-shell/releases/latest
https://github.com/CikeyQi/unidbg-fetch-qsign-shell/releases/latest/download/unidbg-fetch-qsign-1.1.9.zip
https://github.com/CikeyQi/unidbg-fetch-qsign-shell/releases/latest/download/unidbg-fetch-qsign-all.jar
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
        curl -fL --http1.1 --retry 5 --retry-delay 3 --retry-all-errors --continue-at - --connect-timeout 15 --output "$output" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -c -O "$output" --tries=5 --timeout=30 "$url"
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
    asset_url="$(sed -n 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\.zip\)".*/\1/p' "$json_path" | head -n 1)"
    if [ -z "$asset_url" ]; then
        asset_url="$(sed -n 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\.jar\)".*/\1/p' "$json_path" | head -n 1)"
    fi
    rm -f "$json_path"
    [ -n "$asset_url" ] || return 1
    printf '%s\n' "$asset_url"
}

extract_zip() {
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
}

install_txlib_from_extract_dir() {
    target_root="$(txlib_root)"
    find "$EXTRACT_DIR" -type f -name libfekit.so | while IFS= read -r fekit_path; do
        source_path="$(dirname -- "$fekit_path")"
        [ -f "$source_path/config.json" ] || continue
        [ -f "$source_path/dtconfig.json" ] || continue
        target_path="$target_root/$(basename -- "$source_path")"
        mkdir -p "$target_path"
        cp -R "$source_path/." "$target_path/"
        echo "qsign txlib 已安装到 $target_path" >&2
    done
}

install_bin_from_extract_dir() {
    bin_source_path="$(find "$EXTRACT_DIR" -type f -path '*/bin/unidbg-fetch-qsign' | sort | head -n 1)"
    [ -n "$bin_source_path" ] || return 1
    mkdir -p "$(dirname -- "$BIN_PATH")"
    cp "$bin_source_path" "$BIN_PATH"
    chmod +x "$BIN_PATH"
    echo "qsign 可执行文件已安装到 $BIN_PATH" >&2
}

install_distribution_from_extract_dir() {
    distribution_bin="$(find "$EXTRACT_DIR" -type f -path '*/bin/unidbg-fetch-qsign' | sort | head -n 1)"
    [ -n "$distribution_bin" ] || return 1
    distribution_root="$(dirname -- "$(dirname -- "$distribution_bin")")"
    [ -d "$distribution_root/lib" ] || return 1

    mkdir -p "$ROOT_DIR/bin" "$ROOT_DIR/lib"
    cp -R "$distribution_root/bin/." "$ROOT_DIR/bin/"
    cp -R "$distribution_root/lib/." "$ROOT_DIR/lib/"
    chmod +x "$BIN_PATH"
    echo "qsign 分发目录已安装到 $ROOT_DIR" >&2
}

downloaded_jar_is_ready() {
    downloaded_path="$1"
    unzip -p "$downloaded_path" META-INF/MANIFEST.MF 2>/dev/null | grep -q '^Main-Class: MainKt'
}

install_downloaded_qsign() {
    downloaded_path="$1"

    if file -b "$downloaded_path" 2>/dev/null | grep -qi 'java archive\|jar'; then
        downloaded_jar_is_ready "$downloaded_path" || {
            echo "qsign 下载到的 jar 不是可运行服务主程序。" >&2
            return 1
        }
        return 0
    fi

    if downloaded_jar_is_ready "$downloaded_path"; then
        return 0
    fi

    if file -b "$downloaded_path" 2>/dev/null | grep -qi 'zip archive'; then
        extract_zip "$downloaded_path"
        install_txlib_from_extract_dir || true
        if install_distribution_from_extract_dir; then
            rm -f "$downloaded_path"
            return 0
        fi
        install_bin_from_extract_dir || true
        if bin_path_is_ready; then
            rm -f "$downloaded_path"
            return 0
        fi
    fi

    echo "qsign 下载文件不是 jar，也不是包含 jar 的 zip。" >&2
    return 1
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

install_downloaded_qsign "$TMP_PATH"

if [ -f "$TMP_PATH" ] && [ -n "$SHA256" ]; then
    ACTUAL_SHA256="$(sha256sum "$TMP_PATH" | awk '{print $1}')"
    if [ "$ACTUAL_SHA256" != "$SHA256" ]; then
        echo "qsign 下载文件 SHA256 不匹配。" >&2
        echo "期望: $SHA256" >&2
        echo "实际: $ACTUAL_SHA256" >&2
        exit 1
    fi
fi

if [ -f "$TMP_PATH" ]; then
    mv -f "$TMP_PATH" "$JAR_PATH"
    echo "qsign 已安装到 $JAR_PATH" >&2
fi