# Changelog

All notable changes to GetPaper are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).
Until 1.0.0, minor versions may contain breaking changes; they will always be listed here.

Docker images are published as `ghcr.io/teopost/getpaper:<version>`.

## [Unreleased]

### Added

- **Template variables and placeholders**, as in Outline: `{date}`, `{time}`, `{datetime}` and `{author}` in the title
  and text of a template are replaced when a new document is created from it (also through MCP); placeholders
  (`!!text!!`, or the placeholder button in a template's formatting menu) are highlighted in the new document and
  replaced by clicking and typing.

### Changed

- The "Workspaces" link at the bottom of the sidebar is removed: the workspace settings are in the menu that opens
  from the workspace name.

### Fixed

- The email sent to people who already have an account and are added to a workspace now opens that workspace,
  instead of the last one they used.
- When inviting people who already have an account, the invite window says so by name: they are added right away,
  no invite is created.

## [0.1.0] - 2026-10-17

First public release.

### Workspaces and organisation

- Multiple fully isolated workspaces, each with its own members, roles, collections, documents, shares and API keys;
  workspace switcher and workspace logo.
- Collections with icon and colour and "edit", "read only" or "private" permissions with explicit members.
- Nested documents with drag and drop, breadcrumbs, and a sidebar that keeps only the current collection open.
- Move documents between workspaces together with their subpages, history, comments and attachments.
- Members and roles (admin, editor, viewer), invites by link or email, groups with read or edit access to collections.

### Editor

- WYSIWYG editor saved as Markdown: `/` menu, formatting toolbar, Markdown shortcuts, tables with resizable columns,
  checklists, notices, images and attachments by paste or drag.
- Code blocks with syntax highlighting, language picker, copy and line wrapping.
- Mermaid diagrams and a table of contents block.
- Mentions of people, groups, documents and collections, with notifications in the app and by email.
- Autosave with conflict detection, drafts, revision history with restore, templates, comments with replies,
  stars, archive and a 30-day trash.
- Presentation mode.

### Search and sharing

- Full-text search in PostgreSQL with highlighted results, a collection filter and search across all workspaces.
- Public read-only links (`noindex`, `no-referrer`), with an indicator on public documents; sharing can be disabled
  per workspace or per collection.

### Sign-in and administration

- Email and password sign-in, invite-only mode.
- Microsoft Entra ID single sign-on (OpenID Connect, authorization code flow with PKCE), with tenant and email domain
  restrictions, automatic or invite-only provisioning and an SSO-only mode; can reuse the redirect URI of an Outline
  instance.
- Invitation and notification emails through SMTP.
- Superadmin settings: product name and colours, switches for backups, Markdown export and public links, and default
  language, theme and page width for new users.
- Interface in English and Italian; light, dark or automatic theme.

### Import and export

- Import of Markdown, HTML and text files or ZIP archives; Markdown exports from Outline, Notion, Obsidian, Wiki.js,
  GitBook and BookStack.
- Import of Confluence space exports (XML or HTML) with page tree, dates, attachments and the most common macros.
- GetPaper backup of a workspace (documents, history, comments, attachments) and restore on another installation.
- Export of a workspace as an Obsidian vault; single documents as Markdown, HTML, PDF or Word.

### API and AI

- Outline-style RPC API with personal API keys.
- Built-in MCP server (Streamable HTTP) with read-only or read/write keys limited to chosen workspaces and collections,
  with optional expiry.

### Deployment

- Docker image for amd64 and arm64 on GitHub Container Registry and a Compose file for the published image.
- Makefile with the common tasks for installations from source.

[Unreleased]: https://github.com/teopost/getpaper/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/teopost/getpaper/releases/tag/v0.1.0
