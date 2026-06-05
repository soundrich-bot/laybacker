# Laybacker

**Batch replace audio on video files.** A desktop tool for sound post-production: drag in video and audio, Laybacker auto-matches the pairs by duration, checks and fixes loudness to broadcast/online spec, and exports. Free, open-source, runs entirely on your machine.

👉 **[laybacker.com](https://laybacker.com)** · [Download the latest release](https://github.com/soundrich-bot/laybacker/releases/latest)

[![Build & Release](https://github.com/soundrich-bot/laybacker/actions/workflows/build.yml/badge.svg)](https://github.com/soundrich-bot/laybacker/actions/workflows/build.yml)
[![Quality](https://github.com/soundrich-bot/laybacker/actions/workflows/quality.yml/badge.svg)](https://github.com/soundrich-bot/laybacker/actions/workflows/quality.yml)
[![CodeQL](https://github.com/soundrich-bot/laybacker/actions/workflows/codeql.yml/badge.svg)](https://github.com/soundrich-bot/laybacker/actions/workflows/codeql.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/soundrich-bot/laybacker/badge)](https://securityscorecards.dev/viewer/?uri=github.com/soundrich-bot/laybacker)
[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=soundrich-bot_laybacker&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=soundrich-bot_laybacker)
[![codecov](https://codecov.io/gh/soundrich-bot/laybacker/branch/main/graph/badge.svg)](https://codecov.io/gh/soundrich-bot/laybacker)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

## What it does

- **Drag, drop, done** — drop video and audio together; Laybacker matches them by duration and name, and lets you fix any pairing by hand.
- **Broadcast-spec loudness** — normalise to LUFS targets (EBU R128 −23, streaming −16, ATSC A/85) or full-scale peak, measured to ITU-R BS.1770-4.
- **6-frame silence check** — verify and fix head/tail silence for UK commercial broadcast delivery, with clean fades.
- **Smart export** — keep original streams or re-encode to H.264 / AAC, with smart filenames.
- **FFmpeg included** — a pinned, bundled FFmpeg ships inside the app. Nothing else to install.
- **Fully local & private** — every file is processed on your own machine. Nothing is ever uploaded.

## Built with

Tauri 2 · Rust · Svelte 5 · bundled FFmpeg.

## Trust & verification

Laybacker is built to be independently checkable, not just trusted:

- **Open source** — the full source is public, and every installer is built by GitHub Actions from a tagged commit with public build logs.
- **VirusTotal** — every release is scanned by 70+ antivirus engines (links in the release notes).
- **Checksums** — each release publishes `SHA256SUMS` files.
- **Build provenance** — each binary carries a signed [SLSA attestation](https://slsa.dev): `gh attestation verify <file> --repo soundrich-bot/laybacker`.
- **Code quality** — strict Clippy linting, CodeQL scanning, OpenSSF Scorecard, SonarCloud grading, and test coverage (badges above).
- **In-app auto-update** — signed updates, verified before install.

## License

[GPL-3.0](LICENSE). Built on [FFmpeg](https://ffmpeg.org) (GPL). © 2026 Soundrich Ltd.
