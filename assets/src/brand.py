import sys
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

font_path, out_dir = sys.argv[1], sys.argv[2]

INK = "#1C2330"
MARBLE = "#EEF0F2"
LAPIS = ["#2B4BDB", "#3653E0", "#2442C4", "#3A5AE8", "#2847CF", "#3150D9", "#2240BE", "#3352DD"]
VERDIGRIS = "#2E9E8F"

def mark(size=64, tile=18, gap=3, radius=3.2, origin=(3.5, 3.5)):
    rects = []
    k = 0
    for row in range(3):
        for col in range(3):
            x = origin[0] + col * (tile + gap)
            y = origin[1] + row * (tile + gap)
            if (row, col) == (0, 2):
                rects.append(
                    f'<rect x="{x + 2.6:.1f}" y="{y - 2.6:.1f}" width="{tile}" height="{tile}" rx="{radius}" '
                    f'fill="{VERDIGRIS}" transform="rotate(9 {x + 2.6 + tile / 2:.1f} {y - 2.6 + tile / 2:.1f})"/>'
                )
                continue
            turn = [-2.2, 1.6, -1.1, 2.0, -1.6, 1.2, -2.0, 1.4][k]
            cx, cy = x + tile / 2, y + tile / 2
            rects.append(
                f'<rect x="{x:.1f}" y="{y:.1f}" width="{tile}" height="{tile}" rx="{radius}" fill="{LAPIS[k]}" '
                f'transform="rotate({turn} {cx:.1f} {cy:.1f})"/>'
            )
            k += 1
    return "\n  ".join(rects)

def wordmark_paths(text, height):
    var = TTFont(font_path)
    inst = instantiateVariableFont(var, {"wdth": 125, "wght": 760})
    glyphs = inst.getGlyphSet()
    cmap = inst.getBestCmap()
    units = inst["head"].unitsPerEm
    ascent = inst["OS/2"].sCapHeight or inst["hhea"].ascent
    xheight = inst["OS/2"].sxHeight
    scale = height / xheight
    pen = SVGPathPen(glyphs)
    x = 0
    tracking = -10
    for ch in text:
        name = cmap[ord(ch)]
        glyph = glyphs[name]
        tp = TransformPen(pen, (scale, 0, 0, -scale, x * scale, xheight * scale))
        glyph.draw(tp)
        x += glyph.width + tracking
    return pen.getCommands(), x * scale, xheight * scale

def write(path, body):
    with open(path, "w") as f:
        f.write(body)

m = mark()
write(f"{out_dir}/mark.svg", f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 70 70" width="70" height="70" role="img" aria-label="Tessera">
  {m}
</svg>
''')

d, width, h = wordmark_paths("tessera", 34)
for name, fill in (("light", INK), ("dark", MARBLE)):
    write(f"{out_dir}/logo-{name}.svg", f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {96 + width:.0f} 70" width="{(96 + width) * 1.6:.0f}" height="112" role="img" aria-label="Tessera">
  {m}
  <path transform="translate(88 {(70 - h) / 2 + 2:.1f})" fill="{fill}" d="{d}"/>
</svg>
''')
print("wordmark width", round(width), "height", round(h))
