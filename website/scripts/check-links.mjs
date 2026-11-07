// Fails on dead relative links.
//
// Two checks, so a moved or renamed page cannot leave a broken link behind:
//   1. In the built site (`dist/`): every internal link and image resolves to a
//      file, and every `#fragment` names an id on that page.
//   2. In the Markdown sources (`docs/`, `ROADMAP.md`, `README.md`): every
//      relative link resolves to a file or directory in the repository. The
//      sync step turns such links into GitHub URLs, which no build can check.
//
// Run after `npm run build`: `node scripts/check-links.mjs`.
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..", "..");
const dist = process.env.TESSERA_DOCS_DIST ?? path.resolve(import.meta.dirname, "..", "dist");
const base = "/tessera";
const problems = [];

function walk(dir, keep) {
  const found = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) found.push(...walk(full, keep));
    else if (keep(full)) found.push(full);
  }
  return found;
}

// --- 1. The built site -------------------------------------------------------

if (!fs.existsSync(dist)) {
  console.error(`No built site at ${dist}; run \`npm run build\` first.`);
  process.exit(2);
}

const idsCache = new Map();
function idsOf(file) {
  if (!idsCache.has(file)) {
    const html = fs.readFileSync(file, "utf8");
    idsCache.set(file, new Set([...html.matchAll(/\s(?:id|name)="([^"]+)"/g)].map((m) => m[1])));
  }
  return idsCache.get(file);
}

/** The file a site path serves, or null. */
function served(sitePath) {
  const target = path.join(dist, decodeURIComponent(sitePath));
  for (const candidate of [target, path.join(target, "index.html"), `${target}.html`]) {
    if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) return candidate;
  }
  return null;
}

let checkedLinks = 0;
for (const page of walk(dist, (f) => f.endsWith(".html"))) {
  const html = fs.readFileSync(page, "utf8");
  const pageUrl = "/" + path.relative(dist, page).replace(/index\.html$/, "").replace(/\\/g, "/");
  // A redirect stub holds one meta refresh and no content; check where it goes.
  const refresh = html.match(/<meta http-equiv="refresh" content="[^"]*url=([^"]+)"/);
  if (refresh && !/<main/.test(html)) {
    checkedLinks++;
    const to = refresh[1];
    if (!to.startsWith(`${base}/`) || !served(to.slice(base.length))) {
      problems.push(`${pageUrl}: redirects to "${to}", which is not in the built site`);
    }
    continue;
  }
  for (const [, , raw] of html.matchAll(/\s(href|src)="([^"]*)"/g)) {
    if (/^(https?:|mailto:|tel:|data:|javascript:)/.test(raw) || raw === "") continue;
    const [target, fragment] = raw.split("#");
    let sitePath;
    if (target === "") sitePath = pageUrl;
    else if (target.startsWith("/")) {
      if (!(target === base || target.startsWith(`${base}/`))) {
        problems.push(`${pageUrl}: "${raw}" does not start with ${base}/`);
        continue;
      }
      sitePath = target.slice(base.length) || "/";
    } else sitePath = new URL(target, `http://x${pageUrl}`).pathname;
    checkedLinks++;
    const file = served(sitePath.split("?")[0]);
    if (!file) {
      problems.push(`${pageUrl}: "${raw}" points at nothing in the built site`);
      continue;
    }
    if (fragment && file.endsWith(".html") && !idsOf(file).has(decodeURIComponent(fragment))) {
      problems.push(`${pageUrl}: "${raw}" names an anchor that is not on the page`);
    }
  }
}

// --- 2. Links from the Markdown sources into the repository --------------------

const sources = [
  ...walk(path.join(root, "docs"), (f) => f.endsWith(".md")),
  path.join(root, "ROADMAP.md"),
  path.join(root, "README.md"),
];
let checkedSources = 0;
for (const source of sources) {
  const text = fs.readFileSync(source, "utf8").replace(/```[\s\S]*?```/g, "");
  for (const [, raw] of text.matchAll(/\]\(([^)\s]+)\)/g)) {
    if (/^(https?:|mailto:|#)/.test(raw)) continue;
    const [file] = raw.split("#");
    const resolved = path.resolve(path.dirname(source), file);
    checkedSources++;
    if (!fs.existsSync(resolved)) {
      problems.push(`${path.relative(root, source)}: "${raw}" points at nothing in the repository`);
    }
  }
}

if (problems.length > 0) {
  console.error(`${problems.length} dead link(s):\n` + problems.map((p) => `  ${p}`).join("\n"));
  process.exit(1);
}
console.log(`links ok: ${checkedLinks} in the built site, ${checkedSources} in the Markdown sources`);
