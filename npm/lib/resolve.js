"use strict";

const PACKAGES = {
  "darwin-arm64": "tessera-codegraph-darwin-arm64",
  "darwin-x64": "tessera-codegraph-darwin-x64",
  "linux-x64-musl": "tessera-codegraph-linux-x64-musl",
  "linux-arm64-musl": "tessera-codegraph-linux-arm64-musl",
  "win32-x64": "tessera-codegraph-win32-x64",
};

function currentHost() {
  return { platform: process.platform, arch: process.arch };
}

// Linux always gets the static musl build: it runs on every distribution,
// including glibc versions older than the ones release runners link against.
function platformPackage(host = currentHost()) {
  let key = `${host.platform}-${host.arch}`;
  if (host.platform === "linux") key += "-musl";
  const name = PACKAGES[key];
  if (!name) {
    throw new Error(
      `no prebuilt tessera binary for ${host.platform}/${host.arch}; ` +
        "install with `cargo install tessera-codegraph` instead"
    );
  }
  return name;
}

function resolveBinary(host = currentHost(), resolve = require.resolve) {
  const name = platformPackage(host);
  const exe = host.platform === "win32" ? "tessera.exe" : "tessera";
  try {
    return resolve(`${name}/bin/${exe}`);
  } catch (error) {
    throw new Error(
      `Install failed: package ${name} is missing. ` +
        "Reinstall without --no-optional / --omit=optional, or use `cargo install tessera-codegraph`."
    );
  }
}

module.exports = { PACKAGES, platformPackage, resolveBinary };
