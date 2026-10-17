# GetPaper - instructions for Claude

GetPaper is an open-source (MIT), self-hosted wiki: Rust backend in `server/`, React + TipTap frontend in `web/`,
website in `docs/` (GitHub Pages, https://getpaper.it). Contribution rules, layout and commands are in
@CONTRIBUTING.md; follow them.

## Language and style

- Write commits, release notes, `CHANGELOG.md`, code comments and documentation in **English**: the public repository
  is read mostly by English speakers.
- Keep the Italian translations in sync when you change the English text: `README.md` → `README_ITA.md`,
  `docs/index.html` → `docs/it/index.html`, `web/src/locales/en` → `web/src/locales/it`.
- Use the plain hyphen "-", never the em dash or en dash, in any text.
- Commit messages: a short sentence that describes the change from the user's point of view, then details if needed.

## Accuracy

- Never claim a feature, guarantee, number or integration that you have not verified in the code. This applies to the
  README, the website, `docs/llms.txt`, release notes and the GitHub repository description.
- Do not invent users, stars, customers, testimonials or benchmarks.

## Keeping things in sync

For every user-visible change, in the same commit or pull request:

- add a line to the `Unreleased` section of `CHANGELOG.md`;
- update `README.md` and `README_ITA.md` if behaviour or configuration changes;
- update the website (`docs/index.html`, `docs/it/index.html`) and `docs/llms.txt` when a headline feature changes.

## Publishing and releases

- `main` is pushed to `origin` (`make push`) and then published to the public repository `teopost/getpaper` with
  `make publish`, which rewrites author emails, so **public commit hashes differ from local ones**.
- Releases follow the `/release` skill (`.claude/skills/release/SKILL.md`): changelog, version bump, tag `vX.Y.Z`,
  GitHub release with English notes on the public commit, check of the Docker image build.
- Pushing, publishing, tagging and creating releases are visible to everyone: do them only when the maintainer asks.

## Checks

- Backend tests: `cd server && cargo test`. The image must build: `docker compose build`.
- Website: check desktop, tablet and phone widths, light and dark themes, no horizontal scroll, no console errors.
- The Docker data disk on the maintainer's server is small: remove images and containers you pull for testing when
  you are done, and check free space (`docker system df`) before large builds.
