#!/usr/bin/env node
// Thin launcher: exec the prebuilt binary that postinstall placed next to this
// file, forwarding argv, stdio, and exit code.
const path = require("path");
const fs = require("fs");
const { spawnSync } = require("child_process");

const exe = process.platform === "win32" ? "tessera.exe" : "tessera";
const bin = path.join(__dirname, exe);

if (!fs.existsSync(bin)) {
  console.error(
    "[tessera] binary not found — the postinstall download may have failed.\n" +
      "Reinstall, or install via `cargo install tessera-codegraph`."
  );
  process.exit(1);
}

const res = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (res.error) {
  console.error(`[tessera] failed to launch: ${res.error.message}`);
  process.exit(1);
}
process.exit(res.status === null ? 1 : res.status);
