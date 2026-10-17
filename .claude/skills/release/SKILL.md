---
name: release
description: Publish a new GetPaper release - changelog, version bump, tag, push, public mirror, GitHub release notes in English and Docker image check. Use only when the maintainer asks for a release.
disable-model-invocation: true
argument-hint: "[version, e.g. 0.2.0]"
---

# Release GetPaper

Publish version `$ARGUMENTS` (if empty, propose the next version from the `Unreleased` changes using Semantic
Versioning: until 1.0.0, new features bump the minor version, fixes the patch version). Write everything in English.

## 1. Preconditions

- Working tree clean and on `main`, up to date with `origin/main` (`git status -sb`, `git fetch origin`).
- `git config getpaper.publish.email` and `git config getpaper.publish.rewrite` are set; if not, stop and ask the
  maintainer for the values (they are local and never committed).
- Git author identity is set (`git config user.name` / `user.email`); if not, ask which identity to use.
- `gh auth status` works for the `teopost` account.
- The tag `vX.Y.Z` does not exist yet, locally or on GitHub.
- The `Unreleased` section of `CHANGELOG.md` is not empty. If it is, build it from `git log <last tag>..HEAD`,
  describing only changes you can verify in the diff.

## 2. Prepare the release commit

1. In `CHANGELOG.md`, move the `Unreleased` entries under `## [X.Y.Z] - YYYY-MM-DD` (today), leave an empty
   `## [Unreleased]`, and update the compare links at the bottom:
   `[Unreleased]: .../compare/vX.Y.Z...HEAD` and `[X.Y.Z]: .../releases/tag/vX.Y.Z`.
2. Set the version in `server/Cargo.toml` (`version = "X.Y.Z"`), `server/Cargo.lock` (the `getpaper` package
   entry) and `web/package.json` (and the root entries of `web/package-lock.json`).
3. Update version examples if they mention an older release: `README.md`, `README_ITA.md`, `docs/index.html`,
   `docs/it/index.html`, `docs/llms.txt`.
4. Run the checks from `CLAUDE.md` (at least `cargo test` and `docker compose build` if disk space allows). If a
   check cannot run, say so in the final report.
5. Commit: `Release X.Y.Z`, then an annotated tag: `git tag -a vX.Y.Z -m "GetPaper X.Y.Z"`.

## 3. Push and publish

```bash
git push origin main
git push origin vX.Y.Z
make publish                 # pushes main to teopost/getpaper with rewritten emails
git fetch public
```

The public commit has a different hash. Create the public tag and release on it:

```bash
SHA=$(git rev-parse public/main)          # must be the "Release X.Y.Z" commit
gh release create vX.Y.Z -R teopost/getpaper --target "$SHA" \
  --title "GetPaper X.Y.Z" --notes-file <notes.md> --latest
```

## 4. Release notes (`notes.md`, English)

- One sentence on what the release is about.
- **Upgrade**: `docker compose pull && docker compose up -d`, or pin `GETPAPER_IMAGE=ghcr.io/teopost/getpaper:X.Y.Z`;
  call out breaking changes, new required variables and database migrations, and recommend a backup first.
- **Highlights**: the user-visible changes from the changelog, grouped (Added, Changed, Fixed).
- **Known limitations** if any changed.
- Link to the full `CHANGELOG.md` and to the documentation.
- Only verified facts; no em dash.

## 5. Verify

- `gh run list -R teopost/getpaper` → the "Docker image" run for the tag `vX.Y.Z` succeeds
  (`gh run watch <id> --exit-status`).
- `docker manifest inspect ghcr.io/teopost/getpaper:X.Y.Z` lists `amd64` and `arm64`; `:X.Y` exists too.
- The release page shows the notes and is marked as latest.
- The GitHub Pages deployment for the new commit succeeds and https://getpaper.it responds.

Report what was released, the links (release, image), the checks that passed and any that were skipped.
