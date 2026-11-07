#!/bin/sh
# Tessera installer.
#   curl -fsSL https://raw.githubusercontent.com/iamsaquib8/tessera/main/install.sh | sh
#
# Environment:
#   TESSERA_VERSION      version to install, with or without the leading v
#                        (v1.2.0 or 1.2.0; default: the latest release)
#   TESSERA_INSTALL_DIR  where to put the binary (default: $HOME/.local/bin)
#   TESSERA_BASE_URL     release download base (default: GitHub releases)
#   TESSERA_LATEST_URL   a URL that redirects to <anything>/tag/<tag> for the
#                        latest release (default: GitHub's releases/latest).
#                        With TESSERA_BASE_URL set (a mirror), "latest" needs
#                        this or an explicit TESSERA_VERSION.
#
# Every download is checked against the release's SHA256SUMS before install.
set -eu

REPO="iamsaquib8/tessera"
BIN="tessera"
INSTALL_DIR="${TESSERA_INSTALL_DIR:-${TESSERA_BIN_DIR:-$HOME/.local/bin}}"
BASE_URL="${TESSERA_BASE_URL:-https://github.com/${REPO}/releases/download}"
if [ -n "${TESSERA_LATEST_URL:-}" ]; then
  LATEST_URL="$TESSERA_LATEST_URL"
elif [ -z "${TESSERA_BASE_URL:-}" ]; then
  LATEST_URL="https://github.com/${REPO}/releases/latest"
else
  LATEST_URL=""
fi

say() { printf 'tessera-install: %s\n' "$1" >&2; }
die() { say "error: $1"; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1"; }

need uname
need tar
if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL "$1" -o "$2"; }
  # Where `$1` redirects to, without following it.
  redirect_of() { curl -fsSI -o /dev/null -w '%{redirect_url}' "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -qO "$2" "$1"; }
  redirect_of() {
    wget -qS --spider --max-redirect=0 "$1" 2>&1 |
      sed -n 's/^ *[Ll]ocation: *//p' | head -n 1 | tr -d '\r'
  }
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
  *) die "unsupported OS: $os (download a release archive from https://github.com/iamsaquib8/tessera/releases, or run: cargo install tessera-codegraph)" ;;
esac

version="${TESSERA_VERSION:-latest}"
if [ "$version" = "latest" ]; then
  # GitHub redirects /releases/latest to /releases/tag/<tag>. Unlike the REST
  # API, the redirect has no rate limit for anonymous clients.
  [ -n "$LATEST_URL" ] ||
    die "TESSERA_BASE_URL is set, so \"latest\" cannot be resolved; set TESSERA_VERSION (or TESSERA_LATEST_URL)"
  location="$(redirect_of "$LATEST_URL" || true)"
  case "$location" in
    */tag/*) version="${location##*/tag/}" ;;
    *) die "could not find the latest release at ${LATEST_URL}; set TESSERA_VERSION=vX.Y.Z" ;;
  esac
  version="${version%%[?#]*}"
  [ -n "$version" ] || die "could not determine the latest version; set TESSERA_VERSION=vX.Y.Z"
fi
# Release tags start with v, but 1.2.0 is the natural way to write a version.
case "$version" in
  v*) ;;
  *) version="v${version}" ;;
esac

archive="${BIN}-${target}.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading ${BIN} ${version} for ${target}"
fetch "${BASE_URL}/${version}/${archive}" "$tmp/$archive" ||
  die "download failed: ${BASE_URL}/${version}/${archive} (is ${version} a released version? see https://github.com/${REPO}/releases)"
fetch "${BASE_URL}/${version}/SHA256SUMS" "$tmp/SHA256SUMS" ||
  die "could not download SHA256SUMS for ${version}; refusing to install unverified binary"

expected="$(awk -v f="$archive" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")"
[ -n "$expected" ] || die "SHA256SUMS has no entry for ${archive}"
actual="$(sha256 "$tmp/$archive")"
[ "$expected" = "$actual" ] || die "checksum mismatch for ${archive} (expected ${expected}, got ${actual})"

tar -xzf "$tmp/$archive" -C "$tmp" || die "could not extract ${archive}"
bin_path="$(find "$tmp" -type f -name "$BIN" | head -n 1)"
[ -n "$bin_path" ] || die "${archive} does not contain ${BIN}"

mkdir -p "$INSTALL_DIR" || die "could not create ${INSTALL_DIR}"
# Copy beside the target, then rename: a running tessera keeps its old inode,
# and macOS never sees a signed binary rewritten in place.
staged="$INSTALL_DIR/.${BIN}.new.$$"
cp "$bin_path" "$staged" || die "could not write to ${INSTALL_DIR}"
chmod 0755 "$staged"
mv -f "$staged" "$INSTALL_DIR/$BIN" || { rm -f "$staged"; die "could not replace ${INSTALL_DIR}/${BIN}"; }
say "installed ${INSTALL_DIR}/${BIN} (sha256 verified)"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) say "note: add ${INSTALL_DIR} to your PATH: export PATH=\"${INSTALL_DIR}:\$PATH\"" ;;
esac
"$INSTALL_DIR/$BIN" --version || true
