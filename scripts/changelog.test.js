"use strict";
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { main, body, problems, fragments } = require("./changelog.js");

function repo(files, changelog = "# Changelog\n\nIntro.\n\n## 1.0.0 - 2026-01-01\n\n- Old.\n") {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "changelog-"));
  fs.mkdirSync(path.join(root, "changes"));
  fs.writeFileSync(path.join(root, "CHANGELOG.md"), changelog);
  for (const [name, text] of Object.entries(files)) {
    fs.writeFileSync(path.join(root, "changes", name), text);
  }
  return root;
}
const quiet = (fn) => {
  const { log, error } = console;
  console.log = console.error = () => {};
  try {
    return fn();
  } finally {
    Object.assign(console, { log, error });
  }
};

test("entries are grouped by kind, in the documented order", () => {
  const root = repo({
    "b.fixed.md": "Fix B.\n",
    "a.added.md": "Add A,\nover two lines.\n",
    "c.added.md": "Add C.\n",
  });
  assert.strictEqual(
    body(fragments(path.join(root, "changes"))),
    "### Added\n\n- Add A,\n  over two lines.\n- Add C.\n\n### Fixed\n\n- Fix B."
  );
});

test("release inserts the section above the previous one and removes the fragments", () => {
  const root = repo({ "a.added.md": "Add A.\n", "README.md": "ignored" });
  assert.strictEqual(quiet(() => main(["release", "1.1.0", "2026-02-02"], root)), 0);
  const text = fs.readFileSync(path.join(root, "CHANGELOG.md"), "utf8");
  assert.ok(text.indexOf("## 1.1.0 - 2026-02-02") < text.indexOf("## 1.0.0"), text);
  assert.ok(text.includes("### Added\n\n- Add A."), text);
  assert.deepStrictEqual(fs.readdirSync(path.join(root, "changes")), ["README.md"]);
});

test("release refuses a version that already has a section, and keeps the fragments", () => {
  const root = repo({ "a.added.md": "Add A.\n" });
  assert.throws(() => quiet(() => main(["release", "1.0.0", "2026-02-02"], root)), /already has/);
  assert.strictEqual(fs.readdirSync(path.join(root, "changes")).length, 1);
});

test("check rejects bad names, empty text, leading bullets and headings", () => {
  const list = [
    { name: "x.md", slug: "", kind: "x", text: "t" },
    { name: "a.fixed.md", slug: "a", kind: "fixed", text: "" },
    { name: "b.fixed.md", slug: "b", kind: "fixed", text: "- bullet" },
    { name: "c.fixed.md", slug: "c", kind: "fixed", text: "## heading" },
  ];
  assert.strictEqual(problems(list).length, 4);
  assert.strictEqual(problems([{ name: "a.fixed.md", slug: "a", kind: "fixed", text: "ok" }]).length, 0);
});

test("preview and release need fragments", () => {
  const root = repo({});
  assert.strictEqual(quiet(() => main(["preview"], root)), 1);
  assert.strictEqual(quiet(() => main(["release", "1.1.0"], root)), 1);
  assert.strictEqual(quiet(() => main(["release", "one"], repo({ "a.added.md": "A." }))), 2);
});
