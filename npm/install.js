#!/usr/bin/env node
// postinstall: download the prebuilt `tessera` binary matching this platform
// from the matching GitHub release and place it in bin/. No Rust toolchain
// needed. The launcher (bin/tessera.js) execs whatever this drops here.
const fs = require("fs");
const path = require("path");
const os = require("os");
const { spawnSync } = require("child_process");

const REPO = "iamsaquib8/tessera";
const pkg = require("./package.json");
const VERSION = process.env.TESSERA_VERSION || `v${pkg.version}`;

function target() {
  const platform = os.platform();
  const arch = os.arch();
  const archPart = arch === "arm64" ? "aarch64" : arch === "x64" ? "x86_64" : null;
  if (!archPart) return null;
  if (platform === "linux") return { triple: `${archPart}-unknown-linux-gnu`, ext: "tar.gz", exe: "" };
  if (platform === "darwin") return { triple: `${archPart}-apple-darwin`, ext: "tar.gz", exe: "" };
  if (platform === "win32") return { triple: `${archPart}-pc-windows-msvc`, ext: "zip", exe: ".exe" };
  return null;
}

async function main() {
  const t = target();
  const binDir = path.join(__dirname, "bin");
  fs.mkdirSync(binDir, { recursive: true });
  const dest = path.join(binDir, `tessera${t ? t.exe : ""}`);

  if (!t) {
    console.warn(
      `[tessera] no prebuilt binary for ${os.platform()}/${os.arch()}. ` +
        `Install via \`cargo install tessera-codegraph\` instead.`
    );
    return;
  }

  const archive = `tessera-${t.triple}.${t.ext}`;
  const url = `https://github.com/${REPO}/releases/download/${VERSION}/${archive}`;
  const tmp = path.join(os.tmpdir(), archive);

  console.log(`[tessera] downloading ${archive} (${VERSION})`);
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) {
    console.warn(
      `[tessera] download failed (${res.status}) from ${url}. ` +
        `Try \`cargo install tessera-codegraph\` or download from the Releases page.`
    );
    return;
  }
  const buf = Buffer.from(await res.arrayBuffer());
  fs.writeFileSync(tmp, buf);

  // bsdtar (`tar`) handles both .tar.gz and .zip on Linux/macOS/Windows 10+.
  const r = spawnSync("tar", ["-xf", tmp, "-C", binDir], { stdio: "inherit" });
  if (r.status !== 0) {
    console.warn(`[tessera] could not extract ${archive} (is \`tar\` available?).`);
    return;
  }
  if (fs.existsSync(dest)) fs.chmodSync(dest, 0o755);
  try { fs.unlinkSync(tmp); } catch {}
  console.log(`[tessera] installed ${dest}`);
}

main().catch((e) => {
  console.warn(`[tessera] install error: ${e && e.message}. Falling back to manual install.`);
});
