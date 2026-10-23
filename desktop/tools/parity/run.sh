#!/usr/bin/env bash
# Compares native output captures with frozen references.
#
#   tools/parity/run.sh [out-dir]
#
# The lyric and teleprompter references (proj-default*, proj-styled,
# conf-default*) are the React output components rendered in headless
# Chromium on 2026-10-11, the last build that had them. Black, Logo and video
# had no webview reference worth keeping; their references are native
# captures from 2026-10-24, so they catch regressions. Each scenario is
# rendered natively offscreen (`cargo run -p render_engine --example capture
# <scene>`) and compared: prints the share of pixels whose gray level differs
# by more than 25%, writes a red/cyan overlay, and fails if any exceeds its
# tolerance. A deliberate rendering change shows up here; replace the
# reference with the reviewed native capture when it is intended.
#
# Needs ImageMagick. Captures use system fonts, so run on a machine with the
# fonts the references were made with (see docs/records/2026-10-native-output-migration.md, stages 4 and 5).
set -euo pipefail
here=$(dirname "$(readlink -f "$0")")
desktop=$(readlink -f "$here/../..")
out=$(readlink -f "${1:-$here/out}")

# name scene width height tolerance(%)
# Tolerances sit just above the baseline measured on 2026-10-10 (0.27, 0.48,
# 1.01, 0.80, 2.67). The small confidence scene differs most: Chromium rounds
# its scroll position and row metrics to whole pixels, which moves every row
# about 1 px against the native layout.
scenarios=(
  "proj-default proj-default 1920 1080 1"
  "proj-default-43 proj-default 1440 1080 1"
  "proj-styled proj-styled 1920 1080 1.5"
  "conf-default conf-default 1920 1080 1.5"
  "conf-default-small conf-default 960 540 3.5"
  "proj-black proj-black 960 540 0.1"
  "proj-logo proj-logo 960 540 0.5"
  "proj-logo-fallback proj-logo-fallback 960 540 0.5"
  "proj-video proj-video 960 540 0.5"
  "conf-video conf-video 960 540 0.5"
)

mkdir -p "$out/native" "$out/compare"
cargo build --quiet --manifest-path "$desktop/Cargo.toml" -p render_engine --example capture

failed=0
for scenario in "${scenarios[@]}"; do
  read -r name scene width height tolerance <<<"$scenario"
  cargo run --quiet --manifest-path "$desktop/Cargo.toml" -p render_engine --example capture -- \
    "$scene" "$width" "$height" "$out/native/$name.png"
  reference=$here/reference/$name.png native=$out/native/$name.png
  percent=$(magick \( "$reference" -colorspace gray \) \( "$native" -colorspace gray \) \
    -compose difference -composite -threshold 25% -format '%[fx:mean*100]' info:)
  # Reference in red, native in green and blue: matches are gray, mismatches red or cyan.
  magick \( "$reference" -colorspace gray \) \( "$native" -colorspace gray \) \( "$native" -colorspace gray \) \
    -combine "$out/compare/$name.png"
  verdict=ok
  if python3 -c "import sys; sys.exit(0 if float(sys.argv[1]) <= float(sys.argv[2]) else 1)" "$percent" "$tolerance"; then :; else verdict=FAIL; failed=1; fi
  printf '%-20s %5sx%-5s %6.2f%% differ  %s\n' "$name" "$width" "$height" "$percent" "$verdict"
done
echo "Overlays: $out/compare"
exit $failed
