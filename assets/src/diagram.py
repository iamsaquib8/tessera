"""Render the README "how it works" diagram in light and dark variants.

Usage: python3 assets/src/diagram.py assets
"""
import sys

OUT = sys.argv[1]
THEMES = {
    "light": {"bg": "#EEF0F2", "panel": "#FFFFFF", "ink": "#1C2330", "dim": "#5B6677", "line": "#C5CCD6"},
    "dark": {"bg": "#161C27", "panel": "#1F2736", "ink": "#EEF0F2", "dim": "#9AA6B8", "line": "#36425A"},
}
LAPIS, VERDIGRIS = "#2B4BDB", "#2E9E8F"

STEPS = [
    ("Before the edit", ["tessera blast <symbol>", "tessera context <symbol>"], "Who is affected, ranked,", "and which tests to run", LAPIS),
    ("Your agent edits", ["writes code", "calls project APIs"], "Any agent, any model", "Claude, GPT, Gemini, Grok…", None),
    ("After the edit", ["tessera verify --snippet …", "exit 1 on a broken call"], "Invented APIs are caught", "with the closest real name", VERDIGRIS),
]

W, H = 960, 350
COL_W, COL_H, GAP, TOP, LEFT = 280, 170, 40, 28, 20

for name, t in THEMES.items():
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" role="img" '
        f'aria-label="Before an edit Tessera ranks the blast radius; after it, verify checks every call; both read a local index.">',
        f'<rect width="{W}" height="{H}" rx="14" fill="{t["bg"]}"/>',
        '<g font-family="system-ui, -apple-system, Segoe UI, Helvetica, Arial, sans-serif">',
    ]
    for i, (title, code, line1, line2, accent) in enumerate(STEPS):
        x = LEFT + i * (COL_W + GAP)
        stroke = accent or t["line"]
        parts.append(f'<rect x="{x}" y="{TOP}" width="{COL_W}" height="{COL_H}" rx="10" fill="{t["panel"]}" stroke="{stroke}" stroke-width="1.5"/>')
        if accent:
            parts.append(f'<rect x="{x + 20}" y="{TOP + 22}" width="12" height="12" rx="2.5" fill="{accent}" transform="rotate(6 {x + 26} {TOP + 28})"/>')
        tx = x + (44 if accent else 20)
        parts.append(f'<text x="{tx}" y="{TOP + 33}" font-size="17" font-weight="700" fill="{t["ink"]}">{title}</text>')
        for j, c in enumerate(code):
            mono = c.startswith("tessera")
            family = ' font-family="JetBrains Mono, SFMono-Regular, Menlo, Consolas, monospace"' if mono else ""
            parts.append(f'<text x="{x + 20}" y="{TOP + 72 + j * 24}" font-size="{14 if mono else 15}"{family} fill="{t["ink"]}">{c.replace("<", "&lt;").replace(">", "&gt;")}</text>')
        parts.append(f'<text x="{x + 20}" y="{TOP + 132}" font-size="14" fill="{t["dim"]}">{line1}</text>')
        parts.append(f'<text x="{x + 20}" y="{TOP + 152}" font-size="14" fill="{t["dim"]}">{line2}</text>')
        if i < len(STEPS) - 1:
            ax = x + COL_W + 6
            ay = TOP + COL_H / 2
            parts.append(f'<path d="M{ax} {ay} h{GAP - 16}" stroke="{t["dim"]}" stroke-width="1.5"/>')
            parts.append(f'<path d="M{ax + GAP - 18} {ay - 5} l7 5 l-7 5" fill="none" stroke="{t["dim"]}" stroke-width="1.5"/>')
    by = TOP + COL_H + 34
    parts.append(f'<rect x="{LEFT}" y="{by}" width="{W - 2 * LEFT}" height="96" rx="10" fill="{t["panel"]}" stroke="{t["line"]}" stroke-width="1.5"/>')
    for k in range(6):
        parts.append(f'<rect x="{LEFT + 22 + k * 15}" y="{by + 28}" width="11" height="11" rx="2" fill="{LAPIS if k != 4 else VERDIGRIS}" opacity="{0.55 + 0.07 * k:.2f}"/>')
    parts.append(f'<text x="{LEFT + 128}" y="{by + 33}" font-size="16" font-weight="700" fill="{t["ink"]}">Local index</text>')
    parts.append(f'<text x="{LEFT + 128}" y="{by + 56}" font-size="14" fill="{t["dim"]}">tree-sitter → SQLite in .tessera/, built on first use and refreshed on every change.</text>')
    parts.append(f'<text x="{LEFT + 128}" y="{by + 76}" font-size="14" fill="{t["dim"]}">Same code, same answer. Nothing leaves your machine.</text>')
    for i in (0, 2):
        cx = LEFT + i * (COL_W + GAP) + COL_W / 2
        parts.append(f'<path d="M{cx} {TOP + COL_H + 2} V{by - 2}" stroke="{t["line"]}" stroke-width="1.5" stroke-dasharray="4 4"/>')
    parts.append("</g></svg>")
    with open(f"{OUT}/how-it-works-{name}.svg", "w") as f:
        f.write("\n".join(parts) + "\n")
