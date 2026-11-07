// Copies the Markdown in ../docs (and the roadmap pages kept elsewhere in the
// repository) into Starlight's content folder, so the repository stays the
// single source of truth.
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..", "..");
const docs = path.join(root, "docs");
const out = process.env.TESSERA_DOCS_OUT ?? path.join(import.meta.dirname, "..", "src", "content", "docs");
const repo = "https://github.com/iamsaquib8/tessera/blob/main";

// The version every page says it describes: the release being published (set
// by the Pages workflow), else the crate version of this checkout.
const cargoVersion = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8").match(/^version = "([^"]+)"/m)[1];
const version = process.env.TESSERA_DOCS_VERSION || `${cargoVersion} (this checkout)`;
const banner = `These docs describe Tessera ${version}.`;

const pages = {
  "quickstart.md": "Install Tessera, connect your agent with tessera init, and see it check an edit.",
  "guides/index.md": "Pick the task you are trying to do; each guide has the commands and real output.",
  "guides/check-before-you-edit.md": "See who a change affects before you or your agent make it.",
  "guides/catch-what-an-agent-broke.md": "Find the calls and imports an edit broke, and hold the agent until they are fixed.",
  "guides/run-only-affected-tests.md": "Run the tests a change can reach, locally and in CI.",
  "guides/block-breaking-api-changes.md": "Stop a removed route, RPC or topic that another service still uses.",
  "guides/review-a-pull-request.md": "Get a deterministic report of what a pull request breaks.",
  "guides/adopt-on-a-legacy-repo.md": "Turn the check on without failing on problems that were already there.",
  "guides/monorepo-and-several-repos.md": "Use Tessera across a monorepo, or across repositories that call each other.",
  "guides/understand-a-new-codebase.md": "Find your way around code you did not write.",
  "guides/connect-your-agent.md": "Wire Tessera into the agents you use, and see what each one gets.",
  "integrations.md": "Connect Tessera to 21 coding agents.",
  "hooks.md": "Checks before and after every edit, and at the end of each turn.",
  "ci.md": "Test selection, index cache, review formats and baselines in any CI.",
  "contracts.md": "Routes, gRPC, GraphQL, topics, tables and config keys across services.",
  "configuration.md": "What gets indexed, and how to change it.",
  "troubleshooting.md": "Fixes for the problems status reports.",
  "mcp.md": "Protocol versions, transports, and the eleven tools.",
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

/** The URL path of a synced page, without the base: `guides/index.md` is `guides`. */
const urlOf = (file) => file.replace(/\.md$/, "").replace(/(^|\/)index$/, "");

/** Links to synced pages point at the site; the rest at the repository. */
function rewriteLinks(markdown, fromDir, fromFile) {
  return markdown.replace(/\]\(([^)\s]+)\)/g, (match, target) => {
    if (/^(https?:|mailto:|#)/.test(target)) return match;
    const [file, anchor = ""] = target.split("#");
    const hash = anchor ? `#${anchor}` : "";
    const resolved = path.resolve(fromDir, file);
    const page = sources.get(resolved);
    if (page) {
      const relative = path.posix.relative(urlOf(fromFile) || ".", urlOf(page[0]) || ".");
      return `](${relative ? `${relative}/` : "./"}${hash})`;
    }
    return `](${repo}/${path.relative(root, resolved)}${hash})`;
  });
}

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });
for (const [sourcePath, [file, description]] of sources) {
  const source = fs.readFileSync(sourcePath, "utf8");
  const [, title = file] = source.match(/^#\s+(.+)$/m) ?? [];
  const body = rewriteLinks(source.replace(/^#\s+.+\n+/, ""), path.dirname(sourcePath), file);
  const frontmatter = `---\ntitle: ${JSON.stringify(title)}\ndescription: ${JSON.stringify(description)}\nbanner:\n  content: ${JSON.stringify(banner)}\n---\n\n`;
  fs.mkdirSync(path.dirname(path.join(out, file)), { recursive: true });
  fs.writeFileSync(path.join(out, file), frontmatter + body);
}
const home = fs.readFileSync(path.join(import.meta.dirname, "..", "src", "index.mdx"), "utf8");
fs.writeFileSync(path.join(out, "index.mdx"), home.replace(/^---\n/, `---\nbanner:\n  content: ${JSON.stringify(banner)}\n`));
console.log(`synced ${sources.size} pages`);
