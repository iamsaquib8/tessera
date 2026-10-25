// Copies the Markdown in ../docs (and the roadmap pages kept elsewhere in the
// repository) into Starlight's content folder, so the repository stays the
// single source of truth.
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..", "..");
const docs = path.join(root, "docs");
const out = path.join(import.meta.dirname, "..", "src", "content", "docs");
const repo = "https://github.com/iamsaquib8/tessera/blob/main";

const pages = {
  "quickstart.md": "Get started in a minute.",
  "first-five-minutes.md": "Look before you edit, check after you edit, with real output.",
  "integrations.md": "Connect Tessera to 24 coding agents.",
  "hooks.md": "Checks before and after every edit, and at the end of each turn.",
  "contracts.md": "Routes, gRPC, GraphQL, topics, tables and config keys across services.",
  "configuration.md": "What gets indexed, and how to change it.",
  "troubleshooting.md": "Fixes for the problems status reports.",
  "mcp.md": "Protocol versions, transports, and the nine tools.",
  "stability.md": "What every 1.x release keeps working.",
  "accuracy.md": "How often each confidence level is right, measured.",
  "agents.md": "Sourced matrix of agent MCP and hook capabilities.",
  "architecture.md": "How indexing, freshness, and queries fit together.",
  "when-not-to-use.md": "Current limits, stated plainly.",
  "how-to-add-a-language.md": "Add or deepen a language extractor.",
  "how-to-add-an-agent.md": "Add an agent integration.",
};

// Pages whose source lives outside docs/: slug → [path from the root, description].
const extra = {
  "roadmap.md": ["ROADMAP.md", "What comes next, and what proves each step done."],
  "landscape.md": [
    "docs/design/2026-10-25-landscape.md",
    "Other tools for agent code context, contracts, tests and guardrails, and what Tessera takes from them.",
  ],
};

// Every synced source, by absolute path, with its slug and description.
const sources = new Map([
  ...Object.entries(pages).map(([file, description]) => [path.join(docs, file), [file, description]]),
  ...Object.entries(extra).map(([file, [source, description]]) => [path.join(root, source), [file, description]]),
]);

/** Links to synced pages point at the site; the rest at the repository. */
function rewriteLinks(markdown, fromDir) {
  return markdown.replace(/\]\(([^)\s]+)\)/g, (match, target) => {
    if (/^(https?:|mailto:|#)/.test(target)) return match;
    const [file, anchor = ""] = target.split("#");
    const hash = anchor ? `#${anchor}` : "";
    const resolved = path.resolve(fromDir, file);
    const page = sources.get(resolved);
    if (page) return `](../${page[0].replace(/\.md$/, "")}/${hash})`;
    return `](${repo}/${path.relative(root, resolved)}${hash})`;
  });
}

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });
for (const [sourcePath, [file, description]] of sources) {
  const source = fs.readFileSync(sourcePath, "utf8");
  const [, title = file] = source.match(/^#\s+(.+)$/m) ?? [];
  const body = rewriteLinks(source.replace(/^#\s+.+\n+/, ""), path.dirname(sourcePath));
  const frontmatter = `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\n---\n\n`;
  fs.writeFileSync(path.join(out, file), frontmatter + body);
}
fs.copyFileSync(path.join(import.meta.dirname, "..", "src", "index.mdx"), path.join(out, "index.mdx"));
console.log(`synced ${sources.size} pages`);
