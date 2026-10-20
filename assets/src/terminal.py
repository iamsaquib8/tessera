"""Render a terminal session SVG from real `tessera` output.

Usage: python3 assets/src/terminal.py <tessera-binary> <repo-dir> > assets/demo.svg
"""
import html
import subprocess
import sys

BIN, REPO = sys.argv[1], sys.argv[2]
SESSION = [
    ("tessera blast formatName", None),
    ("tessera context findById", None),
    ("tessera verify --snippet edit.ts --language typescript", "UserRepo.findByIdd(2);\nloadUsr(3);\n"),
]

INK, TEXT, DIM, BAD, GOOD, PROMPT = "#1C2330", "#C9D1DD", "#8A97AA", "#F07A7A", "#5CCFBE", "#EEF0F2"
LINE, CHAR, PAD_X, PAD_TOP = 22, 8.6, 28, 58


def run(command, stdin):
    if stdin is not None:
        with open(f"{REPO}/edit.ts", "w") as f:
            f.write(stdin)
    args = command.split()[1:]
    out = subprocess.run([BIN, "--quiet", *args], cwd=REPO, capture_output=True, text=True)
    return out.stdout.rstrip("\n").splitlines()


def colour(line):
    if line.startswith("✗") or "[V1]" in line:
        return BAD
    if line.startswith("✓"):
        return GOOD
    if line.startswith(("Calls:", "Called by:", "Tests:")) or "callers affected" in line:
        return DIM
    return TEXT


rows = []
for command, stdin in SESSION:
    rows.append(("$ " + command, PROMPT))
    rows.extend((line, colour(line)) for line in run(command, stdin))
    rows.append(("", TEXT))
rows.pop()

width = int(PAD_X * 2 + CHAR * max(len(text) for text, _ in rows))
height = PAD_TOP + LINE * len(rows) + 24
lines = "\n".join(
    f'  <text x="{PAD_X}" y="{PAD_TOP + LINE * i}" fill="{fill}">{html.escape(text).replace(" ", "\u00a0") or "\u00a0"}</text>'
    for i, (text, fill) in enumerate(rows)
)
print(f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" role="img" aria-label="Terminal session: tessera blast, context and verify">
  <rect width="{width}" height="{height}" rx="12" fill="{INK}"/>
  <rect x="24" y="20" width="11" height="11" rx="3" fill="#2E3A4E"/>
  <rect x="42" y="20" width="11" height="11" rx="3" fill="#2E3A4E"/>
  <rect x="60" y="20" width="11" height="11" rx="3" fill="#2E3A4E"/>
  <g font-family="JetBrains Mono, SFMono-Regular, Menlo, Consolas, monospace" font-size="14.5">
{lines}
  </g>
</svg>''')
