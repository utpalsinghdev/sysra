#!/bin/sh
# sysra installer: downloads the latest release binary for this machine.
#
#   curl -fsSL https://raw.githubusercontent.com/utpalsinghdev/sysra/main/install.sh | sh
#
# Environment:
#   SYSRA_VERSION      tag to install (default: latest release)
#   SYSRA_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#   SYSRA_DOWNLOAD_URL release asset base URL (for mirrors and testing)
set -eu

REPO="utpalsinghdev/sysra"
BIN_DIR="${SYSRA_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '  \033[38;2;139;156;247m•\033[0m %s\n' "$1"; }
die() { printf '  \033[31m✗\033[0m %s\n' "$1" >&2; exit 1; }

fetch() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$2" "$1"
    else
        die "need curl or wget"
    fi
}

case "$(uname -s)" in
    Linux) os="unknown-linux-musl" ;;
    Darwin) os="apple-darwin" ;;
    *) die "unsupported OS: $(uname -s) (build from source: cargo install --git https://github.com/$REPO)" ;;
esac
case "$(uname -m)" in
    x86_64 | amd64) arch="x86_64" ;;
    aarch64 | arm64) arch="aarch64" ;;
    *) die "unsupported CPU: $(uname -m)" ;;
esac
target="$arch-$os"

if [ -n "${SYSRA_DOWNLOAD_URL:-}" ]; then
    base="$SYSRA_DOWNLOAD_URL"
elif [ -n "${SYSRA_VERSION:-}" ]; then
    base="https://github.com/$REPO/releases/download/$SYSRA_VERSION"
else
    base="https://github.com/$REPO/releases/latest/download"
fi
archive="sysra-$target.tar.gz"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
trap 'exit 1' INT TERM

say "downloading sysra for $target"
fetch "$base/$archive" "$tmp/$archive" ||
    die "could not download $archive. Check the release assets or build from source: cargo install --locked --git https://github.com/$REPO"
fetch "$base/$archive.sha256" "$tmp/$archive.sha256" ||
    die "could not download the checksum for $archive"

expected="$(cut -d' ' -f1 < "$tmp/$archive.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$tmp/$archive")"
elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$tmp/$archive")"
else
    die "need sha256sum or shasum to verify the download"
fi
[ "$expected" = "${actual%% *}" ] || die "checksum mismatch for $archive"

tar -xzf "$tmp/$archive" -C "$tmp"
binary="$tmp/sysra-$target/sysra"
version="$("$binary" --version)" || die "downloaded binary could not run"
mkdir -p "$BIN_DIR"
install -m 755 "$binary" "$BIN_DIR/sysra"
say "installed $version to $BIN_DIR/sysra"

case ":$PATH:" in
    *":$BIN_DIR:"*) say "run: sysra" ;;
    *) say "add $BIN_DIR to your PATH, then run: sysra" ;;
esac
