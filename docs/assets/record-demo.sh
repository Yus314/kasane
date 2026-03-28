#!/usr/bin/env bash
set -euo pipefail

# Record and convert Kasane demo for README.
#
# Usage:
#   docs/assets/record-demo.sh              # Record new demo (slurp region select)
#   docs/assets/record-demo.sh --convert    # Convert existing .mp4 to GIF/WebP
#
# Requires: wf-recorder, slurp, ffmpeg
#   nix shell nixpkgs#wf-recorder nixpkgs#slurp --command docs/assets/record-demo.sh
#
# Recording tips (target: ~20 seconds):
#   0. Launch kasane FIRST — kasane docs/assets/demo-theme.md
#   1. Start this script   — select the kasane window interior with slurp
#   2. Navigate down        — show cursor-line highlight + color swatches
#   3. Go to last line      — cursor on ![palette](...) triggers image preview
#   4. Fuzzy finder         — Ctrl+P, type "main", Enter
#   5. Pane split           — Ctrl+W, v
#   6. Hold 2 seconds       — let the final state sink in
#   7. Press Ctrl+C in THIS terminal to stop recording
#
# Tips:
#   - Move the mouse cursor OUT of the kasane window before starting
#   - Wait for plugin init logs to disappear before beginning the demo
#   - Use slurp to select only the terminal content area (no title bar)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RAW_FILE="$SCRIPT_DIR/demo-raw.mp4"
GIF_FILE="$SCRIPT_DIR/demo.gif"
WEBP_FILE="$SCRIPT_DIR/demo.webp"

FPS_RECORD=15
FPS_OUTPUT=12
WIDTH=800

convert_video() {
  if [[ ! -f "$RAW_FILE" ]]; then
    echo "Error: $RAW_FILE not found. Record first." >&2
    exit 1
  fi

  echo "Converting to GIF (${WIDTH}px, ${FPS_OUTPUT}fps, optimized palette)..."
  ffmpeg -y -i "$RAW_FILE" \
    -vf "fps=${FPS_OUTPUT},scale=${WIDTH}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=128:stats_mode=diff[p];[s1][p]paletteuse=dither=floyd_steinberg" \
    -loop 0 "$GIF_FILE" 2>/dev/null
  echo "  → $GIF_FILE ($(du -h "$GIF_FILE" | cut -f1))"

  echo "Converting to WebP (${WIDTH}px, ${FPS_OUTPUT}fps)..."
  ffmpeg -y -i "$RAW_FILE" \
    -vf "fps=${FPS_OUTPUT},scale=${WIDTH}:-1:flags=lanczos" \
    -vcodec libwebp -lossless 0 -compression_level 6 \
    -q:v 50 -loop 0 -an -vsync 0 \
    "$WEBP_FILE" 2>/dev/null
  echo "  → $WEBP_FILE ($(du -h "$WEBP_FILE" | cut -f1))"
}

if [[ "${1:-}" == "--convert" ]]; then
  convert_video
  exit 0
fi

echo "=== Kasane Demo Recording (wf-recorder) ==="
echo ""
echo "Prerequisites:"
echo "  1. Kasane is already running with demo-theme.md"
echo "  2. Mouse cursor is outside the kasane window"
echo "  3. Plugin init logs have cleared"
echo ""
echo "Select the kasane window interior with slurp..."

GEOM=$(slurp)
echo "Region: $GEOM"
echo ""
echo "Recording at ${FPS_RECORD}fps. Press Ctrl+C to stop."
echo ""

wf-recorder -g "$GEOM" -r "$FPS_RECORD" -f "$RAW_FILE"

echo ""
echo "Recording saved to $RAW_FILE"
echo "Converting..."
convert_video

echo ""
echo "Done. Review with: mpv $RAW_FILE"
