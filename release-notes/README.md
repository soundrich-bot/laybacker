# Release notes

Hand-written release notes, one file per version, named after the git tag:

```
release-notes/v0.1.22.md
```

If a file matching the release's tag exists here, the build pipeline
(`.github/workflows/build.yml`) uses its contents verbatim as the **"What's new"**
section — both on the GitHub release page and in the app's in-app update banner
("what's new"). If no file is present, the pipeline falls back to auto-generating
the list from commit subjects since the previous tag.

Keep it short and in plain language — each file is just the body of the notes
(no title needed; the release is already titled with the version). Markdown is
fine. Example:

```markdown
- Laybacker no longer asks for microphone access.
- One audio file can now be laid onto several videos at once.
```
