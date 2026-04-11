#!/bin/bash
# Download FFmpeg and FFprobe static builds for Tauri sidecar bundling.
# Run this before `npm run tauri build` if the binaries directory is empty.
#
# macOS builds from evermeet.cx (x86_64, runs on ARM64 via Rosetta)
# Windows builds from gyan.dev (x86_64)

set -euo pipefail

BINDIR="$(dirname "$0")/../frontend/src-tauri/binaries"
mkdir -p "$BINDIR"

echo "Downloading FFmpeg sidecar binaries..."

# --- macOS ---
if [[ "$(uname)" == "Darwin" ]]; then
  echo "Platform: macOS"

  # Download static builds
  curl -L "https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip" -o /tmp/ffmpeg-mac.zip
  curl -L "https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip" -o /tmp/ffprobe-mac.zip

  unzip -o /tmp/ffmpeg-mac.zip -d /tmp/ffmpeg-mac
  unzip -o /tmp/ffprobe-mac.zip -d /tmp/ffprobe-mac

  # Copy for both architectures (x86_64 runs on ARM64 via Rosetta)
  cp /tmp/ffmpeg-mac/ffmpeg "$BINDIR/ffmpeg-x86_64-apple-darwin"
  cp /tmp/ffmpeg-mac/ffmpeg "$BINDIR/ffmpeg-aarch64-apple-darwin"
  cp /tmp/ffprobe-mac/ffprobe "$BINDIR/ffprobe-x86_64-apple-darwin"
  cp /tmp/ffprobe-mac/ffprobe "$BINDIR/ffprobe-aarch64-apple-darwin"

  chmod +x "$BINDIR"/*-apple-darwin

  rm -rf /tmp/ffmpeg-mac /tmp/ffprobe-mac /tmp/ffmpeg-mac.zip /tmp/ffprobe-mac.zip

  echo "macOS binaries ready:"
  ls -lh "$BINDIR"/*-apple-darwin
fi

# --- Windows (run from WSL or Git Bash) ---
# For Windows ARM64, use x86_64 builds (Windows has built-in x86 emulation)
download_windows() {
  echo "Platform: Windows"
  local FFMPEG_URL="https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"

  curl -L "$FFMPEG_URL" -o /tmp/ffmpeg-win.zip
  unzip -o /tmp/ffmpeg-win.zip -d /tmp/ffmpeg-win

  local FFDIR=$(find /tmp/ffmpeg-win -name "ffmpeg.exe" -path "*/bin/*" | head -1 | xargs dirname)

  cp "$FFDIR/ffmpeg.exe" "$BINDIR/ffmpeg-x86_64-pc-windows-msvc.exe"
  cp "$FFDIR/ffmpeg.exe" "$BINDIR/ffmpeg-aarch64-pc-windows-msvc.exe"
  cp "$FFDIR/ffprobe.exe" "$BINDIR/ffprobe-x86_64-pc-windows-msvc.exe"
  cp "$FFDIR/ffprobe.exe" "$BINDIR/ffprobe-aarch64-pc-windows-msvc.exe"

  rm -rf /tmp/ffmpeg-win /tmp/ffmpeg-win.zip

  echo "Windows binaries ready:"
  ls -lh "$BINDIR"/*-windows-msvc.exe
}

if [[ "${1:-}" == "--windows" ]]; then
  download_windows
fi

echo ""
echo "Done! Binaries are in: $BINDIR"
