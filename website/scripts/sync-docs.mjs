// Copies the Markdown in ../docs into Starlight's content folder so docs/
// stays the single source of truth.
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
  "configuration.md": "What gets indexed, and how to change it.",
  "troubleshooting.md": "Fixes for the problems status reports.",
  "mcp.md": "Protocol versions, transports, and the eight tools.",
  "accuracy.md": "How often each confidence level is right, measured.",
  "agents.md": "Sourced matrix of agent MCP and hook capabilities.",
  "architecture.md": "How indexing, freshness, and queries fit together.",
  "when-not-to-use.md": "Current limits, stated plainly.",
  "how-to-add-a-language.md": "Add or deepen a language extractor.",
  "how-to-add-an-agent.md": "Add an agent integration.",
};

function rewriteLinks(markdown) {
  return markdown.replace(/\]\(([^)\s]+)\)/g, (match, target) => {
    if (/^(https?:|mailto:|#)/.test(target)) return match;
    const [file, anchor = ""] = target.split("#");
    const hash = anchor ? `#${anchor}` : "";
    if (pages[file]) return `](../${file.replace(/\.md$/, "")}/${hash})`;
    const resolved = path.relative(root, path.resolve(docs, file));
    return `](${repo}/${resolved}${hash})`;
  });
}

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });
for (const [file, description] of Object.entries(pages)) {
  const source = fs.readFileSync(path.join(docs, file), "utf8");
  const [, title = file] = source.match(/^#\s+(.+)$/m) ?? [];
  const body = rewriteLinks(source.replace(/^#\s+.+\n+/, ""));
  const frontmatter = `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\n---\n\n`;
  fs.writeFileSync(path.join(out, file), frontmatter + body);
}
fs.copyFileSync(path.join(import.meta.dirname, "..", "src", "index.mdx"), path.join(out, "index.mdx"));
console.log(`synced ${Object.keys(pages).length} pages`);
