# Brand and README assets

Everything in `assets/` is generated from the sources here, so it can be
regenerated after changes.

| Asset | Source | Regenerate |
| --- | --- | --- |
| `mark.svg`, `logo-light.svg`, `logo-dark.svg` | `brand.py` (mosaic mark, plus the Archivo Expanded wordmark converted to outlines) | `python3 assets/src/brand.py <Archivo[wdth,wght].ttf> assets` (needs `fonttools`; the font is from google/fonts, OFL) |
| `demo.svg` | `terminal.py` (runs the real binary on the TypeScript fixture) | `python3 assets/src/terminal.py target/debug/tessera <copy-of-tests/fixtures/typescript> > assets/demo.svg` |
| `how-it-works-light.svg`, `how-it-works-dark.svg` | `diagram.py` | `python3 assets/src/diagram.py assets` |
| `social-preview.png` (1280×640) | `social.html` | Open in Chrome at 1280×640 and screenshot, e.g. `chrome --headless=new --window-size=1280,640 --screenshot=assets/social-preview.png assets/src/social.html` |

Palette:

| Name | Hex | Use |
| --- | --- | --- |
| Marble | `#EEF0F2` | Light background |
| Ink | `#1C2330` | Text and dark background |
| Lapis | `#2B4BDB` | Tiles, links |
| Verdigris | `#2E9E8F` | Verified |
| Cinnabar | `#D64545` | Failures only |

Type: Archivo Expanded (headings and wordmark), Atkinson Hyperlegible
(body), JetBrains Mono (code).
