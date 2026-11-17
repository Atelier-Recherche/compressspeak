#!/usr/bin/env bash
# Copie ffmpeg depuis le PATH vers vendor/ffmpeg/<os>/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OS="$(uname -s)"
case "$OS" in
  Darwin) DEST="$ROOT/vendor/ffmpeg/macos" ;;
  Linux) DEST="$ROOT/vendor/ffmpeg/linux" ;;
  *) echo "OS non supporté: $OS"; exit 1 ;;
esac
mkdir -p "$DEST"
FFMPEG="$(command -v ffmpeg)" || { echo "ffmpeg introuvable dans PATH"; exit 1; }
cp -f "$FFMPEG" "$DEST/ffmpeg"
chmod +x "$DEST/ffmpeg"
if command -v ffprobe >/dev/null; then
  cp -f "$(command -v ffprobe)" "$DEST/ffprobe"
  chmod +x "$DEST/ffprobe"
fi
echo "OK → $DEST"
