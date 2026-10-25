#!/bin/sh
# Publishes a release's npm packages from your machine, for when CI cannot
# (the first publish, before trusted publishing is set up, or an account
# that requires a 2FA code for every publish): npm asks for the code.
#
#   npm login
#   npm/scripts/publish-local.sh v1.0.1
#
# It builds the packages from the GitHub release's archives in a temporary
# checkout of the tag; nothing in this checkout changes.
set -eu

tag="${1:?usage: npm/scripts/publish-local.sh <tag, e.g. v1.0.1>}"
version="${tag#v}"
repo="iamsaquib8/tessera"

for tool in git gh node npm; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool is required" >&2; exit 2; }
done
npm whoami >/dev/null 2>&1 || { echo "error: run \`npm login\` first" >&2; exit 2; }

work=$(mktemp -d)
cleanup() {
  git worktree remove --force "$work/src" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

git fetch --quiet origin "refs/tags/$tag:refs/tags/$tag" 2>/dev/null || true
git worktree add --quiet --detach "$work/src" "$tag"
gh release download "$tag" --repo "$repo" --dir "$work/dist" --pattern 'tessera-*'
node "$work/src/npm/scripts/build-platform-packages.js" "$version" "$work/dist"

# Platform packages first: the launcher depends on them.
for dir in "$work"/src/npm/platforms/*/ "$work/src/npm"; do
  dir="${dir%/}"
  name=$(node -p "require('$dir/package.json').name")
  if npm view "$name@$version" version >/dev/null 2>&1; then
    echo "$name@$version is already published"
  else
    echo "publishing $name@$version"
    npm publish "$dir" --access public
  fi
done
echo "done: npx -y tessera-codegraph@$version --version"
