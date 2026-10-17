# Contributing to GetPaper

Thanks for your interest in GetPaper. Bug reports, ideas, documentation fixes and pull requests are all welcome.

## Reporting bugs and proposing features

- Search the [existing issues](https://github.com/teopost/getpaper/issues) first.
- For a bug, include the GetPaper version (image tag or commit), how you run it, the steps to reproduce, what you
  expected and what happened, plus relevant lines from `docker compose logs app`.
- For a feature, describe the problem you want to solve before the solution. If it is something Outline does,
  say so: GetPaper follows Outline's behaviour where it makes sense.
- **Security issues must not be reported in public issues**: see [SECURITY.md](SECURITY.md).

## Development setup

Requirements: Docker with Compose. For frontend work with hot reload you also need Node.js 22; for backend work
without Docker, a recent stable Rust toolchain.

```bash
git clone https://github.com/teopost/getpaper.git
cd getpaper
docker compose up -d --build        # PostgreSQL + GetPaper on http://localhost:3000
```

Frontend with hot reload (the Vite dev server proxies `/api` and `/files` to `localhost:3000`):

```bash
cd web
npm install
npm run dev
```

Backend tests:

```bash
cd server
cargo test
```

`make` with no arguments lists the common tasks (rebuild, logs, database console, backup and restore).

## Project layout

```
server/             Rust backend (axum + sqlx + PostgreSQL)
server/migrations/  SQL migrations, applied automatically at startup
server/src/api/     RPC API methods, importers, exports and the MCP server
web/                React + TipTap frontend, built with Vite and served by the backend
web/src/locales/    Interface translations (en, it)
deploy/             Compose file for the published Docker image
docs/               Website (GitHub Pages, https://getpaper.it)
```

## Pull requests

- Keep each pull request focused on one change, and open an issue first for larger changes.
- Make sure the image still builds (`docker compose build`) and that `cargo test` passes.
- Database changes go in a new numbered file in `server/migrations/`; never edit a migration that has been released.
- New interface strings need both an English and an Italian translation in `web/src/locales/`.
- Update the documentation in the same pull request: `README.md`, and `README_ITA.md` for the Italian translation.
  Add a line to the `Unreleased` section of [CHANGELOG.md](CHANGELOG.md) for user-visible changes.
- Write code, comments, commit messages and documentation in English.
- Commit messages are a short sentence that describes the change from the user's point of view, for example
  `Resizable table columns, as in Outline`.

## Releases

Releases follow [Semantic Versioning](https://semver.org/). A maintainer moves the `Unreleased` entries in the
changelog under the new version, updates the version in `server/Cargo.toml` and `web/package.json`, and pushes a
`vX.Y.Z` tag; the Docker workflow then publishes `ghcr.io/teopost/getpaper:X.Y.Z` and `:X.Y`.

## License

By contributing you agree that your contributions are released under the [MIT license](LICENSE).
