"use strict";

const PACKAGES = {
  "darwin-arm64": "tessera-codegraph-darwin-arm64",
  "darwin-x64": "tessera-codegraph-darwin-x64",
  "linux-x64-gnu": "tessera-codegraph-linux-x64-gnu",
  "linux-arm64-gnu": "tessera-codegraph-linux-arm64-gnu",
  "linux-x64-musl": "tessera-codegraph-linux-x64-musl",
  "linux-arm64-musl": "tessera-codegraph-linux-arm64-musl",
  "win32-x64": "tessera-codegraph-win32-x64",
};

function hostHasGlibc() {
  if (process.platform !== "linux") return false;
  const report = process.report && process.report.getReport();
  return Boolean(report && report.header && report.header.glibcVersionRuntime);
}

function currentHost() {
  return { platform: process.platform, arch: process.arch, glibc: hostHasGlibc() };
}

function platformPackage(host = currentHost()) {
  let key = `${host.platform}-${host.arch}`;
  if (host.platform === "linux") key += host.glibc ? "-gnu" : "-musl";
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
