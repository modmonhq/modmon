# mm brand assets

The mm mark is the solid block from the terminal interface's header: lowercase `mm`
on lime. Use these files wherever the product needs a logo.

## Files

| File | Use it for |
| --- | --- |
| `mm-mark.svg` | The default logo: lime tile, dark letters. Works on dark and light backgrounds. |
| `mm-mark-inverse.svg` | Dark tile, lime letters. For lime or very bright backgrounds. |
| `mm-mark-mono-black.svg`, `mm-mark-mono-white.svg` | One-color printing, embossing, and watermarks. The letters are cut out of the tile. |
| `mm-icon.svg`, `mm-icon-inverse.svg` | Square tile for favicons, avatars, and app icons. |
| `png/` | The same artwork as transparent PNG files: marks at 256, 512, 1024 and 2048 px wide; icons from 16 to 1024 px. |

Prefer the SVG files. They contain outlined paths, so they need no font.

## Colors

| Name | Hex | RGB |
| --- | --- | --- |
| Lime | `#C6F24E` | 198, 242, 78 |
| Ink | `#0D1203` | 13, 18, 3 |
| Background | `#070908` | 7, 9, 8 |

## Usage

- Keep clear space around the mark of at least the height of one letter.
- Do not use the mark smaller than 20 px wide; use the icon below that size.
- Do not recolor, stretch, rotate, outline, or add effects to the mark.
- In running text, write the product name as `mm`, lowercase.

## Regenerate

The files are generated. Change the geometry in `scripts/generate-brand.mjs`, then run:

```sh
node scripts/generate-brand.mjs --png
```

`--png` renders the PNG and ICO files with Playwright's Chromium, which the dashboard's
development dependencies install (`npm --prefix core/ui ci`, then
`npx --prefix core/ui playwright install chromium`). Without the flag only the SVG
files are written. The script also updates the website icons in `site/public/` and the
mark drawn by the UI kit in `packages/ui/src/brand.ts`.

The letterforms are from Geist Mono Bold, used under the SIL Open Font License.
