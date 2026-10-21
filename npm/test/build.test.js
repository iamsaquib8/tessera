const test = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");

const root = path.join(__dirname, "..");

test("build script unpacks every archive and stamps one version everywhere", () => {
  const work = fs.mkdtempSync(path.join(os.tmpdir(), "tessera-npm-"));
  const npm = path.join(work, "npm");
  fs.cpSync(root, npm, { recursive: true, filter: (src) => !src.includes(`${path.sep}bin${path.sep}tessera`) || src.endsWith(".js") });
  const archives = path.join(work, "archives");
  fs.mkdirSync(archives);
  const targets = JSON.parse(fs.readFileSync(path.join(npm, "platforms", "targets.json"), "utf8"));
  for (const target of Object.values(targets)) {
    const stage = fs.mkdtempSync(path.join(work, "stage-"));
    if (target.includes("windows")) {
      fs.writeFileSync(path.join(stage, "tessera.exe"), "fake");
      execFileSync("zip", ["-q", "-j", path.join(archives, `tessera-${target}.zip`), path.join(stage, "tessera.exe")]);
    } else {
      fs.writeFileSync(path.join(stage, "tessera"), "fake");
      execFileSync("tar", ["-czf", path.join(archives, `tessera-${target}.tar.gz`), "-C", stage, "tessera"]);
    }
  }
  execFileSync("node", [path.join(npm, "scripts", "build-platform-packages.js"), "1.2.3", archives]);
  const main = JSON.parse(fs.readFileSync(path.join(npm, "package.json"), "utf8"));
  assert.strictEqual(main.version, "1.2.3");
  assert.strictEqual(main.scripts, undefined, "no install scripts");
  for (const [platform, target] of Object.entries(targets)) {
    const pkg = JSON.parse(fs.readFileSync(path.join(npm, "platforms", platform, "package.json"), "utf8"));
    assert.strictEqual(pkg.version, "1.2.3");
    assert.strictEqual(main.optionalDependencies[pkg.name], "1.2.3");
    const exe = target.includes("windows") ? "tessera.exe" : "tessera";
    assert.ok(fs.existsSync(path.join(npm, "platforms", platform, "bin", exe)), `${platform} binary`);
  }
});
