#!/usr/bin/env node
"use strict";
// Usage: node scripts/build-platform-packages.js <version> <dir-with-release-archives>
// Stamps <version> into every package.json and unpacks each release archive
// into npm/platforms/<platform>/bin/.

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const [version, archives] = process.argv.slice(2);
if (!version || !archives) {
  console.error("usage: build-platform-packages.js <version> <archives-dir>");
  process.exit(2);
}

const root = path.join(__dirname, "..");
const targets = JSON.parse(fs.readFileSync(path.join(root, "platforms", "targets.json"), "utf8"));

function stamp(file, mutate) {
  const pkg = JSON.parse(fs.readFileSync(file, "utf8"));
  mutate(pkg);
  fs.writeFileSync(file, JSON.stringify(pkg, null, 2) + "\n");
}

for (const [platform, target] of Object.entries(targets)) {
  const dir = path.join(root, "platforms", platform);
  const bin = path.join(dir, "bin");
  fs.rmSync(bin, { recursive: true, force: true });
  fs.mkdirSync(bin, { recursive: true });
  const ext = target.includes("windows") ? "zip" : "tar.gz";
  const archive = path.join(archives, `tessera-${target}.${ext}`);
  if (!fs.existsSync(archive)) {
    console.error(`missing release archive ${archive}`);
    process.exit(1);
  }
  if (ext === "zip") {
    execFileSync("unzip", ["-o", "-q", archive, "-d", bin], { stdio: "inherit" });
  } else {
    execFileSync("tar", ["-xzf", archive, "-C", bin], { stdio: "inherit" });
  }
  const exe = path.join(bin, target.includes("windows") ? "tessera.exe" : "tessera");
  if (!fs.existsSync(exe)) {
    console.error(`archive ${archive} did not contain ${path.basename(exe)}`);
    process.exit(1);
  }
  fs.chmodSync(exe, 0o755);
  stamp(path.join(dir, "package.json"), (pkg) => {
    pkg.version = version;
  });
}

stamp(path.join(root, "package.json"), (pkg) => {
  pkg.version = version;
  for (const name of Object.keys(pkg.optionalDependencies)) {
    pkg.optionalDependencies[name] = version;
  }
});
console.log(`prepared ${Object.keys(targets).length} platform packages at ${version}`);
