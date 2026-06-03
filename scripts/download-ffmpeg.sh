#!/bin/bash
# Download FFmpeg and FFprobe static builds for Tauri sidecar bundling.
# Run this before `npm run tauri build` if the binaries directory is empty.
# CI (.github/workflows/build.yml) runs this automatically before each build.
#
# The FFmpeg version is PINNED below so every build — local or CI, Mac or
# Windows — bundles the same known-good version. Bump FFMPEG_VERSION (and test)
# to move to a newer release deliberately, rather than silently tracking latest.
#
# macOS builds from evermeet.cx (x86_64, runs on ARM64 via Rosetta)
# Windows builds from gyan.dev via GitHub releases (x86_64)

set -euo pipefail

# ── Pinned version ───────────────────────────────────────────────────────────
FFMPEG_VERSION="8.1"

BINDIR="$(dirname "$0")/../frontend/src-tauri/binaries"
mkdir -p "$BINDIR"

echo "Downloading FFmpeg sidecar binaries (pinned to ${FFMPEG_VERSION})..."

# Assert a downloaded binary reports the pinned version. Best-effort on macOS:
# the x86_64 build may need Rosetta to exec on an Apple-Silicon runner, so a
# failure to run is only a warning there, not a hard error.
assert_version() {
  local bin="$1" hard="$2" actual
  if actual="$("$bin" -version 2>/dev/null | head -1)"; then
    case "$actual" in
      *"version ${FFMPEG_VERSION}-"* | *"version ${FFMPEG_VERSION} "*)
        echo "  OK: $actual" ;;
      *)
        echo "  WARNING: expected FFmpeg ${FFMPEG_VERSION}, got: $actual"
        [[ "$hard" == "hard" ]] && { echo "  Aborting (version mismatch)."; exit 1; } ;;
    esac
  else
    echo "  NOTE: could not exec $bin to verify version (Rosetta not present?)"
  fi
}

# --- macOS ---
if [[ "$(uname)" == "Darwin" ]]; then
  echo "Platform: macOS"

  # Prefer the pinned version archive; fall back to the current release if that
  # exact version is no longer archived. The version assertion below guards
  # against accidentally bundling something other than the pin.
  curl -fL "https://evermeet.cx/ffmpeg/ffmpeg-${FFMPEG_VERSION}.zip" -o /tmp/ffmpeg-mac.zip \
    || curl -fL "https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip" -o /tmp/ffmpeg-mac.zip
  curl -fL "https://evermeet.cx/ffmpeg/ffprobe-${FFMPEG_VERSION}.zip" -o /tmp/ffprobe-mac.zip \
    || curl -fL "https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip" -o /tmp/ffprobe-mac.zip

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
  assert_version "$BINDIR/ffmpeg-x86_64-apple-darwin" soft
fi

# --- Windows (run from WSL, Git Bash, or the CI windows runner) ---
# For Windows ARM64, use x86_64 builds (Windows has built-in x86 emulation).
# gyan.dev publishes versioned builds via GitHub releases, so the URL is a
# fully reproducible pin.
download_windows() {
  echo "Platform: Windows"
  local FFMPEG_URL="https://github.com/GyanD/codexffmpeg/releases/download/${FFMPEG_VERSION}/ffmpeg-${FFMPEG_VERSION}-essentials_build.zip"

  curl -fL "$FFMPEG_URL" -o /tmp/ffmpeg-win.zip
  unzip -o /tmp/ffmpeg-win.zip -d /tmp/ffmpeg-win

  local FFDIR
  FFDIR=$(find /tmp/ffmpeg-win -name "ffmpeg.exe" -path "*/bin/*" | head -1 | xargs dirname)

  cp "$FFDIR/ffmpeg.exe" "$BINDIR/ffmpeg-x86_64-pc-windows-msvc.exe"
  cp "$FFDIR/ffmpeg.exe" "$BINDIR/ffmpeg-aarch64-pc-windows-msvc.exe"
  cp "$FFDIR/ffprobe.exe" "$BINDIR/ffprobe-x86_64-pc-windows-msvc.exe"
  cp "$FFDIR/ffprobe.exe" "$BINDIR/ffprobe-aarch64-pc-windows-msvc.exe"

  rm -rf /tmp/ffmpeg-win /tmp/ffmpeg-win.zip

  echo "Windows binaries ready:"
  ls -lh "$BINDIR"/*-windows-msvc.exe
  # On a Windows x86_64 runner the .exe runs natively, so enforce the pin hard.
  assert_version "$BINDIR/ffmpeg-x86_64-pc-windows-msvc.exe" hard
}

if [[ "${1:-}" == "--windows" ]]; then
  download_windows
fi

echo ""
echo "Done! Binaries are in: $BINDIR"
