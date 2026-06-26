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

  # We bundle our OWN lean FFmpeg, built from source by .github/workflows/build-
  # ffmpeg.yml WITHOUT avdevice/audiotoolbox/videotoolbox/network. The binaries
  # therefore link no AVFoundation/CoreAudio/AudioToolbox, so macOS never prompts
  # the user for microphone/camera access — Laybacker only ever reads files.
  # The x86_64 build serves both arch slots (runs on Apple Silicon via Rosetta).
  # Checksums are pinned so a tampered or swapped binary is rejected.
  LEAN_TAG="ffmpeg-lean-8.1"
  BASE="https://github.com/soundrich-bot/laybacker/releases/download/${LEAN_TAG}"
  FFMPEG_SHA256="042c67f1927f1d299128db3a9507b892a4e29f5e59c6e95102ed07c38bd3f59a"
  FFPROBE_SHA256="a3c5dc50e46e3bad9a8d7432283c346cf99dda45144d95b5b5288ec2741b67a0"

  verify_sha256() {
    local file="$1" expected="$2" actual
    actual="$(shasum -a 256 "$file" | awk '{print $1}')"
    if [[ "$actual" != "$expected" ]]; then
      echo "  ERROR: checksum mismatch for $(basename "$file")"
      echo "    expected $expected"
      echo "    got      $actual"
      exit 1
    fi
  }

  for f in ffmpeg-x86_64-apple-darwin ffmpeg-aarch64-apple-darwin; do
    curl -fL "${BASE}/${f}" -o "$BINDIR/$f"
    verify_sha256 "$BINDIR/$f" "$FFMPEG_SHA256"
  done
  for f in ffprobe-x86_64-apple-darwin ffprobe-aarch64-apple-darwin; do
    curl -fL "${BASE}/${f}" -o "$BINDIR/$f"
    verify_sha256 "$BINDIR/$f" "$FFPROBE_SHA256"
  done

  chmod +x "$BINDIR"/*-apple-darwin

  echo "macOS lean binaries ready (checksums verified):"
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
