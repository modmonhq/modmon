#!/usr/bin/env bash
# Turns the HTML snapshots written by the `tui_snapshots` test into PNG images.
#
# 1. Write the snapshots (see CONTRIBUTING.md for running cargo in the container):
#      MM_TUI_SNAPSHOT_DIR=<html-dir> COLORTERM=truecolor \
#        cargo test -p mm --all-features --locked --lib tui_snapshots -- --ignored
#    When cargo runs in a container, the test sees only the container's
#    environment and files: pass both variables to the container and name a
#    directory inside the mounted checkout, for example
#      docker run --rm -e MM_TUI_SNAPSHOT_DIR=/workspace/target/tui-html \
#        -e COLORTERM=truecolor -v "$PWD:/workspace" -w /workspace <rust-image> \
#        cargo test -p mm --all-features --locked --lib tui_snapshots -- --ignored
#    and convert target/tui-html on the host.
# 2. Convert them:
#      scripts/tui-screenshots.sh <html-dir> <png-dir>
# 3. For docs/images/tui, reduce each PNG the docs use and name it after its
#    scene (scene-logs-140x32.png becomes logs.png). This script does not do
#    it. The committed images are 1600 px wide (height in proportion, Lanczos
#    resampling), then reduced to a 256-colour palette without dithering and
#    saved optimized; keep that form so a changed image stays small and
#    compares with the others. With Pillow:
#      image = Image.open(source).convert("RGB")
#      image = image.resize((1600, round(image.height * 1600 / image.width)), Image.LANCZOS)
#      image.quantize(colors=256, method=Image.Quantize.MEDIANCUT,
#                     dither=Image.Dither.NONE).save(target, optimize=True)
#
# Needs a Chromium-based browser: set CHROMIUM, or have chromium, chromium-browser,
# or google-chrome on PATH. A monospace font with box-drawing glyphs must be installed.
set -euo pipefail

if [ "$#" -ne 2 ]; then
    printf 'usage: %s <html-dir> <png-dir>\n' "$0" >&2
    exit 2
fi
source_dir=$(CDPATH= cd -- "$1" && pwd -P)
mkdir -p "$2"
output_dir=$(CDPATH= cd -- "$2" && pwd -P)

browser=${CHROMIUM:-}
if [ -z "$browser" ]; then
    for candidate in chromium chromium-browser google-chrome google-chrome-stable; do
        if command -v "$candidate" >/dev/null 2>&1; then
            browser=$candidate
            break
        fi
    done
fi
if [ -z "$browser" ]; then
    printf 'No Chromium-based browser found; set CHROMIUM to its path.\n' >&2
    exit 2
fi

profile=$(mktemp -d "$output_dir/.chromium-profile.XXXXXX")
trap 'rm -rf "$profile"' EXIT

count=0
for page in "$source_dir"/*.html; do
    [ -e "$page" ] || continue
    name=$(basename "$page" .html)
    # File names end in <columns>x<rows>; size the window to the grid drawn at
    # 14px with a 1.25 line height, plus the page margin.
    size=${name##*-}
    columns=${size%x*}
    rows=${size#*x}
    case "$columns$rows" in
        '' | *[!0-9]*)
            printf 'Skipping %s: its name does not end in <columns>x<rows>.\n' "$name" >&2
            continue
            ;;
    esac
    width=$((columns * 843 / 100 + 34))
    height=$((rows * 35 / 2 + 34))
    "$browser" --headless --disable-gpu --hide-scrollbars --no-first-run \
        --user-data-dir="$profile" --force-device-scale-factor=2 \
        --window-size="$width,$height" --screenshot="$output_dir/$name.png" \
        "file://$page" >/dev/null 2>&1
    count=$((count + 1))
done
printf 'Wrote %s screenshots to %s\n' "$count" "$output_dir"
