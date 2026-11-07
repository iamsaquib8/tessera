#!/usr/bin/env node
// Changelog fragments: one file per change in changes/, assembled into
// CHANGELOG.md when a release is cut, so two pull requests never edit the
// same lines.
//
//   node scripts/changelog.js check                  validate the fragments
//   node scripts/changelog.js preview                print the next release's notes
//   node scripts/changelog.js release <version> [date]
//                                                    write the section into CHANGELOG.md
//                                                    and delete the fragments
//
// A fragment is changes/<slug>.<kind>.md holding the entry's text (no leading
// "- "; continuation lines are fine). Kinds, in the order they appear:
// added, changed, deprecated, removed, fixed, performance, security.
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const KINDS = [
  ["added", "Added"],
  ["changed", "Changed"],
  ["deprecated", "Deprecated"],
  ["removed", "Removed"],
  ["fixed", "Fixed"],
  ["performance", "Performance"],
  ["security", "Security"],
];

function fragments(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((name) => name.endsWith(".md") && name !== "README.md")
    .sort()
    .map((name) => {
      const parts = name.slice(0, -3).split(".");
      return {
        name,
        slug: parts.slice(0, -1).join("."),
        kind: parts[parts.length - 1],
        text: fs.readFileSync(path.join(dir, name), "utf8").trim(),
      };
    });
}

function problems(list) {
  const known = KINDS.map(([kind]) => kind);
  const out = [];
  for (const f of list) {
    if (!f.slug || !known.includes(f.kind)) {
      out.push(`${f.name}: name it <slug>.<kind>.md with kind one of ${known.join(", ")}`);
    }
    if (!f.text) out.push(`${f.name}: empty`);
    if (/^[-*] /.test(f.text)) out.push(`${f.name}: write the text without the leading "- "`);
    if (/^#{1,3} /m.test(f.text)) out.push(`${f.name}: no headings inside an entry`);
  }
  return out;
}

function bullet(text) {
  const [first, ...rest] = text.split("\n");
  return [`- ${first}`, ...rest.map((line) => (line ? `  ${line}` : ""))].join("\n");
}

function body(list) {
  const sections = [];
  for (const [kind, heading] of KINDS) {
    const entries = list.filter((f) => f.kind === kind);
    if (entries.length) {
      sections.push(`### ${heading}\n\n${entries.map((f) => bullet(f.text)).join("\n")}`);
    }
  }
  return sections.join("\n\n");
}

function release(changelog, version, date, list) {
  const heading = `## ${version} - ${date}`;
  if (changelog.split("\n").some((l) => l === `## ${version}` || l.startsWith(`## ${version} `))) {
    throw new Error(`CHANGELOG.md already has a section for ${version}`);
  }
  const lines = changelog.split("\n");
  const at = lines.findIndex((l) => l.startsWith("## "));
  const section = `${heading}\n\n${body(list)}\n`;
  if (at < 0) return `${changelog.trimEnd()}\n\n${section}`;
  return [...lines.slice(0, at), ...section.split("\n"), ...lines.slice(at)].join("\n");
}

function main(argv, root) {
  const [command, version, date] = argv;
  const dir = path.join(root, "changes");
  const list = fragments(dir);
  const bad = problems(list);
  if (bad.length) {
    console.error(bad.join("\n"));
    return 1;
  }
  if (command === "check") {
    console.log(`${list.length} changelog fragment(s) ok`);
    return 0;
  }
  if (command === "preview") {
    if (!list.length) {
      console.error("changes/ has no fragments");
      return 1;
    }
    console.log(body(list));
    return 0;
  }
  if (command === "release") {
    if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
      console.error("usage: changelog.js release <version, e.g. 1.3.0> [YYYY-MM-DD]");
      return 2;
    }
    if (!list.length) {
      console.error("changes/ has no fragments to release");
      return 1;
    }
    const file = path.join(root, "CHANGELOG.md");
    const day = date || new Date().toISOString().slice(0, 10);
    fs.writeFileSync(file, release(fs.readFileSync(file, "utf8"), version, day, list));
    for (const f of list) fs.unlinkSync(path.join(dir, f.name));
    console.log(`CHANGELOG.md: ## ${version} - ${day} from ${list.length} fragment(s)`);
    return 0;
  }
  console.error("usage: changelog.js check | preview | release <version> [date]");
  return 2;
}

if (require.main === module) {
  process.exit(main(process.argv.slice(2), path.resolve(__dirname, "..")));
}

module.exports = { fragments, problems, body, release, main };
