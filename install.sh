#!/bin/sh
# Tessera installer.
#   curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh
#
# Environment:
#   TESSERA_VERSION      tag to install, e.g. v0.10.0 (default: latest release)
#   TESSERA_INSTALL_DIR  where to put the binary (default: $HOME/.local/bin)
#   TESSERA_BASE_URL     release download base (default: GitHub releases)
#
# Every download is checked against the release's SHA256SUMS before install.
set -eu

REPO="iamsaquib8/tessera"
BIN="tessera"
INSTALL_DIR="${TESSERA_INSTALL_DIR:-${TESSERA_BIN_DIR:-$HOME/.local/bin}}"
BASE_URL="${TESSERA_BASE_URL:-https://github.com/${REPO}/releases/download}"

say() { printf 'tessera-install: %s\n' "$1" >&2; }
die() { say "error: $1"; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }

need uname
need tar
if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO "$2" "$1"; }
else
  die "need curl or wget"
fi

if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d ' ' -f 1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
else
  die "need sha256sum or shasum to verify the download"
fi

os="$(uname -s)"
arch="$(uname -m)"
case "$arch" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) die "unsupported architecture: $arch (try: cargo install tessera-codegraph)" ;;
esac
case "$os" in
  # Static musl builds run on any Linux distribution, glibc or not.
  Linux) target="${arch}-unknown-linux-musl" ;;
  Darwin) target="${arch}-apple-darwin" ;;
  *) die "unsupported OS: $os (try: npm i -g tessera-codegraph, or cargo install tessera-codegraph)" ;;
esac

version="${TESSERA_VERSION:-latest}"
if [ "$version" = "latest" ]; then
  tmp_json="$(mktemp)"
  fetch "https://api.github.com/repos/${REPO}/releases/latest" "$tmp_json" ||
    die "could not query the latest release; set TESSERA_VERSION=vX.Y.Z"
  version="$(sed -n 's/.*"tag_name" *: *"\([^"]*\)".*/\1/p' "$tmp_json" | head -n 1)"
  rm -f "$tmp_json"
  [ -n "$version" ] || die "could not determine the latest version; set TESSERA_VERSION=vX.Y.Z"
fi

archive="${BIN}-${target}.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading ${BIN} ${version} for ${target}"
fetch "${BASE_URL}/${version}/${archive}" "$tmp/$archive" ||
  die "download failed: ${BASE_URL}/${version}/${archive}"
fetch "${BASE_URL}/${version}/SHA256SUMS" "$tmp/SHA256SUMS" ||
  die "could not download SHA256SUMS for ${version}; refusing to install unverified binary"

expected="$(awk -v f="$archive" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")"
[ -n "$expected" ] || die "SHA256SUMS has no entry for ${archive}"
actual="$(sha256 "$tmp/$archive")"
[ "$expected" = "$actual" ] || die "checksum mismatch for ${archive} (expected ${expected}, got ${actual})"

tar -xzf "$tmp/$archive" -C "$tmp" || die "could not extract ${archive}"
bin_path="$(find "$tmp" -type f -name "$BIN" | head -n 1)"
[ -n "$bin_path" ] || die "${archive} does not contain ${BIN}"

mkdir -p "$INSTALL_DIR"
cp "$bin_path" "$INSTALL_DIR/$BIN"
chmod 0755 "$INSTALL_DIR/$BIN"
say "installed ${INSTALL_DIR}/${BIN} (sha256 verified)"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) say "note: add ${INSTALL_DIR} to your PATH: export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
esac
"$INSTALL_DIR/$BIN" --version || true
