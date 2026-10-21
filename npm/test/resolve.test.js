const test = require("node:test");
const assert = require("node:assert");
const { platformPackage, resolveBinary } = require("../lib/resolve");

test("maps each supported platform to its package", () => {
  const cases = [
    [{ platform: "darwin", arch: "arm64" }, "tessera-codegraph-darwin-arm64"],
    [{ platform: "darwin", arch: "x64" }, "tessera-codegraph-darwin-x64"],
    [{ platform: "linux", arch: "x64", glibc: true }, "tessera-codegraph-linux-x64-musl"],
    [{ platform: "linux", arch: "arm64", glibc: true }, "tessera-codegraph-linux-arm64-musl"],
    [{ platform: "linux", arch: "x64", glibc: false }, "tessera-codegraph-linux-x64-musl"],
    [{ platform: "linux", arch: "arm64", glibc: false }, "tessera-codegraph-linux-arm64-musl"],
    [{ platform: "win32", arch: "x64" }, "tessera-codegraph-win32-x64"],
  ];
  for (const [host, expected] of cases) {
    assert.strictEqual(platformPackage(host), expected);
  }
});

test("rejects unsupported platforms with a clear message", () => {
  assert.throws(
    () => platformPackage({ platform: "aix", arch: "ppc64" }),
    /no prebuilt tessera binary for aix\/ppc64/
  );
});

test("reports which platform package is missing", () => {
  assert.throws(
    () => resolveBinary({ platform: "linux", arch: "x64", glibc: false }, () => {
      throw new Error("Cannot find module");
    }),
    /Install failed: package tessera-codegraph-linux-x64-musl is missing/
  );
});

test("resolves the binary path inside the platform package", () => {
  const seen = [];
  const bin = resolveBinary({ platform: "win32", arch: "x64" }, (request) => {
    seen.push(request);
    return `/fake/${request}`;
  });
  assert.deepStrictEqual(seen, ["tessera-codegraph-win32-x64/bin/tessera.exe"]);
  assert.strictEqual(bin, "/fake/tessera-codegraph-win32-x64/bin/tessera.exe");
});

test("linux packages install on every libc", () => {
  for (const arch of ["x64", "arm64"]) {
    const pkg = require(`../platforms/linux-${arch}-musl/package.json`);
    assert.strictEqual(pkg.libc, undefined, `linux-${arch}-musl must not be limited to musl hosts`);
  }
});
