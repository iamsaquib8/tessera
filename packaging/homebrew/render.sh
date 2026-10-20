#!/bin/sh
# Usage: render.sh <version-without-v> <SHA256SUMS>  > tessera.rb
set -eu
version="$1"
sums="$2"
template="$(dirname "$0")/tessera.rb.tmpl"

sha_for() {
  sha="$(awk -v f="tessera-$1.tar.gz" '$2 == f || $2 == "*" f { print $1 }' "$sums")"
  [ -n "$sha" ] || { echo "render.sh: no checksum for $1" >&2; exit 1; }
  printf '%s' "$sha"
}

sed \
  -e "s/{{VERSION}}/${version}/g" \
  -e "s/{{SHA_AARCH64_APPLE_DARWIN}}/$(sha_for aarch64-apple-darwin)/" \
  -e "s/{{SHA_X86_64_APPLE_DARWIN}}/$(sha_for x86_64-apple-darwin)/" \
  -e "s/{{SHA_AARCH64_UNKNOWN_LINUX_MUSL}}/$(sha_for aarch64-unknown-linux-musl)/" \
  -e "s/{{SHA_X86_64_UNKNOWN_LINUX_MUSL}}/$(sha_for x86_64-unknown-linux-musl)/" \
  "$template"
