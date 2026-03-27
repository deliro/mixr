#!/bin/sh
set -eu

REPO="deliro/mixr"
BINARY="mixr"
INSTALL_DIR="${MIXR_INSTALL_DIR:-${HOME}/.local/bin}"

die() {
    printf "error: %s\n" "$1" >&2
    exit 1
}

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os_part="unknown-linux-gnu" ;;
        Darwin) os_part="apple-darwin" ;;
        *)      die "unsupported OS: $os" ;;
    esac

    case "$arch" in
        x86_64|amd64)   arch_part="x86_64" ;;
        aarch64|arm64)  arch_part="aarch64" ;;
        *)              die "unsupported architecture: $arch" ;;
    esac

    echo "${arch_part}-${os_part}"
}

fetch_latest_tag() {
    url="https://api.github.com/repos/${REPO}/releases/latest"
    if command -v curl > /dev/null 2>&1; then
        tag=$(curl -fsSL "$url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')
    elif command -v wget > /dev/null 2>&1; then
        tag=$(wget -qO- "$url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')
    else
        die "curl or wget is required"
    fi
    [ -z "$tag" ] && die "could not determine latest release"
    echo "$tag"
}

download() {
    url="$1"
    out="$2"
    if command -v curl > /dev/null 2>&1; then
        curl -fsSL -o "$out" "$url"
    else
        wget -qO "$out" "$url"
    fi
}

confirm_update() {
    current="$1"
    latest="$2"
    printf "%s is already installed (current: %s, latest: %s)\n" "$BINARY" "$current" "$latest"
    printf "update? [y/N] "
    read -r answer
    case "$answer" in
        y|Y|yes|Yes) return 0 ;;
        *) printf "cancelled\n"; exit 0 ;;
    esac
}

strip_v() {
    echo "$1" | sed 's/^v//'
}

main() {
    target="$(detect_target)"
    tag="$(fetch_latest_tag)"
    latest="$(strip_v "$tag")"

    if command -v "$BINARY" > /dev/null 2>&1; then
        current="$("$BINARY" --version 2>/dev/null | awk '{print $2}')"
        if [ "$current" = "$latest" ]; then
            printf "%s %s is already up to date\n" "$BINARY" "$current"
            exit 0
        fi
        confirm_update "$current" "$latest"
    fi

    archive="${BINARY}-${target}.tar.gz"
    url="https://github.com/${REPO}/releases/download/${tag}/${archive}"
    checksum_url="${url}.sha256"

    printf "installing %s %s (%s)\n" "$BINARY" "$tag" "$target"

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    download "$url" "${tmpdir}/${archive}"
    download "$checksum_url" "${tmpdir}/${archive}.sha256"

    printf "verifying checksum... "
    cd "$tmpdir"
    if command -v sha256sum > /dev/null 2>&1; then
        sha256sum -c "${archive}.sha256" > /dev/null 2>&1
    elif command -v shasum > /dev/null 2>&1; then
        shasum -a 256 -c "${archive}.sha256" > /dev/null 2>&1
    else
        printf "skipped (no sha256sum/shasum)\n"
        cd - > /dev/null
        tar xzf "${tmpdir}/${archive}" -C "$tmpdir"
        install_binary "$tmpdir"
        return
    fi
    printf "ok\n"
    cd - > /dev/null

    tar xzf "${tmpdir}/${archive}" -C "$tmpdir"
    install_binary "$tmpdir"
}

install_binary() {
    srcdir="$1"
    mkdir -p "$INSTALL_DIR"
    install -m 755 "${srcdir}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
    printf "%s installed to %s/%s\n" "$BINARY" "$INSTALL_DIR" "$BINARY"

    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *) printf "WARNING: %s is not in PATH — add it to your shell profile:\n  export PATH=\"%s:\$PATH\"\n" "$INSTALL_DIR" "$INSTALL_DIR" ;;
    esac
}

main
