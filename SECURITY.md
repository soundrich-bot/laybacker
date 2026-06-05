# Security Policy

## Reporting a vulnerability

If you find a security issue in Laybacker, please report it **privately** so it
can be fixed before public disclosure:

- **Email:** soundrich+laybacker@gmail.com, or
- Use GitHub's **Security → Report a vulnerability** on this repository.

Please include the affected version and steps to reproduce. We aim to respond
within a few days.

## Supported versions

The latest released version receives security fixes. The app also updates
itself in-app, so please update to the newest release before reporting.

## How releases are verified

Every release is built by GitHub Actions from a tagged commit, scanned by
VirusTotal, published with SHA-256 checksums, and carries a signed
[SLSA](https://slsa.dev) build-provenance attestation. The codebase is checked
on every change by CodeQL, OpenSSF Scorecard, SonarCloud, and Clippy. See the
[README](README.md) for details.
