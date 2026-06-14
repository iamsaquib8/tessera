#!/bin/sh
# Tessera installer.  Usage:
#   curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh
#
# Env overrides:
#   TESSERA_VERSION   tag to install (default: latest release)
#   TESSERA_BIN_DIR   install directory (default: $HOME/.local/bin)
set -eu

REPO="iamsaquib8/tessera"
BIN="tessera"
BIN_DIR="${TESSERA_BIN_DIR:-$HOME/.local/bin}"

say() { printf 'tessera-install: %s\n' "$1" >&2; }
die() { say "error: $1"; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }

need uname
need tar
if command -v curl >/dev/null 2>&1; then
  dl() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  dl() { wget -qO "$2" "$1"; }
else
  die "need curl or wget"
fi

os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux)  os_part="unknown-linux-gnu" ;;
  Darwin) os_part="apple-darwin" ;;
  *) die "unsupported OS: $os (use cargo/npm/docker instead)" ;;
esac
case "$arch" in
  x86_64|amd64) arch_part="x86_64" ;;
  arm64|aarch64) arch_part="aarch64" ;;
  *) die "unsupported architecture: $arch" ;;
esac
target="${arch_part}-${os_part}"

version="${TESSERA_VERSION:-latest}"
if [ "$version" = "latest" ]; then
  # Resolve the latest tag via the GitHub API.
  api="https://api.github.com/repos/${REPO}/releases/latest"
  tmp_json="$(mktemp)"
  dl "$api" "$tmp_json" || die "could not query latest release"
  version="$(grep -m1 '"tag_name"' "$tmp_json" | sed -E 's/.*"tag_name" *: *"([^"]+)".*/\1/')"
  rm -f "$tmp_json"
  [ -n "$version" ] || die "could not determine latest version"
fi

archive="${BIN}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/download/${version}/${archive}"
say "downloading ${BIN} ${version} (${target})"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
dl "$url" "$tmp/$archive" || die "download failed: $url"
tar -xzf "$tmp/$archive" -C "$tmp" || die "extract failed"

mkdir -p "$BIN_DIR"
# The archive may contain the binary at the root or under a target dir.
bin_path="$(find "$tmp" -name "$BIN" -type f -perm -u+x 2>/dev/null | head -n1)"
[ -n "$bin_path" ] || bin_path="$(find "$tmp" -name "$BIN" -type f 2>/dev/null | head -n1)"
[ -n "$bin_path" ] || die "binary not found in archive"
install -m 0755 "$bin_path" "$BIN_DIR/$BIN" 2>/dev/null || { cp "$bin_path" "$BIN_DIR/$BIN"; chmod 0755 "$BIN_DIR/$BIN"; }

say "installed to $BIN_DIR/$BIN"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) say "note: $BIN_DIR is not on your PATH — add: export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac
"$BIN_DIR/$BIN" --version || true
