# Changelog

All notable changes to GetPaper are documented in this file. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).
Until 1.0.0, minor versions may contain breaking changes; they will always be listed here.

Docker images are published as `ghcr.io/teopost/getpaper:<version>`.

## [Unreleased]

### Added

- **More ways to sign in**, all off until configured: Google, Slack and any OpenID Connect provider (`GOOGLE_*`,
  `SLACK_*`, `OIDC_*`; accounts linked by verified email), a sign-in link by email (`EMAIL_LOGIN=true`, needs
  SMTP) and passkeys (`PASSKEYS=true`, added in the profile). Microsoft sign-in and its configuration are unchanged.
- **Webhooks** (Settings → Webhooks, admins): the events of the workspace, all or by group, as signed HTTP POST
  requests (`X-GetPaper-Signature`, HMAC-SHA256), with retries, a delivery log and a test button; only public
  addresses unless `WEBHOOKS_ALLOW_PRIVATE=true`.
- **Notification settings** (Settings → Notifications): an email preference for each kind of notification, which
  replaces the two switches of the profile (their values are kept). New notifications: comments on followed
  documents and replies in your threads, resolved threads, access to a collection, accepted invitations.
- **Pinned documents** at the top of the home page (admins) and of a collection (who can edit it), reordered by
  dragging.
- **Backlinks**: *Referenced by* under a document lists the documents that link to it or mention it.
- **Public link options**: include the nested documents (with a tree on the public page), allow search engine
  indexing (with a sitemap), show or hide the date of the last change. Existing links keep working as before.
- **Markdown source of a document**: *Edit as Markdown* in the document menu shows the text as Markdown, editable,
  and switches back to the visual editor; viewers get *Show Markdown*, read only.
- Database migrations 0017-0022: share options, notification settings, pins, document links (computed for the
  existing documents), webhooks, sign-in identities, sign-in links and passkeys.

- **Suspending members and accounts**: workspace admins suspend a member (no access to the workspace until
  reactivated; role, groups, permissions and documents are kept), superadmins suspend an account in the whole
  installation (no sign-in, sessions closed, API keys stop working). Suspended people can't be given access.
- **Sharing a document with people or groups**, as in Outline: Share → People with access, "can edit" or "view
  only", inherited by nested documents; shared documents appear in a new "Shared with me" section of the sidebar,
  in search and in the API.
- **Following documents and collections**: notifications (and emails, with a new profile preference) when a
  followed document has a new editing session, a document is published in a followed collection or a document is
  shared with you; publishing or editing a document follows it; repeated updates refresh one unread notification.
- **Comments on a passage of the text**, also by readers: the passage is highlighted, the thread shows the quote
  and clicking it goes to the text; the Markdown of the document is not changed.
- **Embeds** of YouTube, Vimeo, Loom, Figma, Google Docs, Sheets, Slides, Forms and Drive, Miro, Spotify and
  CodePen (pasting the link on an empty line, or choosing the service in the / menu, which then asks for its link
  as in Outline), stored as a link; **link previews** for documents of the
  workspace and, if a superadmin enables them, for external sites (fetched by the server, public addresses only).

- **Activity and audit log**. Workspace admins have Settings → Activity: people (last activity, sign-ins, documents
  published and edited, optionally documents viewed), changes to documents from the revision history, and an audit
  log of sign-ins and failed sign-ins, invites, roles, groups, collection access, documents published, moved,
  archived, restored or deleted, public links, API keys, imports, exports and workspace settings, with IP address,
  browser and channel (web, API key, MCP), filters and CSV export. Superadmins have the audit log of the
  installation (sign-ins, accounts, general settings, superadmins, workspaces). Drafts are not recorded. Kept for
  90 days by default, configurable in the general settings (0 turns it off); documents viewed per person are off by
  default. Database migration: new `events` table.

### Fixed

- **Import from Wiki.js**: exports whose `pages.json.gz` lacks the comma between some pages, as Wiki.js writes it
  when it exports the pages in batches, are imported instead of failing with "Cannot read pages.json.gz"; pages
  that the export contains more than once are imported once, in their most recent version.
- **Import from Wiki.js**: diagrams (base64 SVG in a ```` ```diagram ```` block) and images embedded as `data:` URIs
  become attachments next to the page. Pages with several MB of them made the import fail with "Internal error"
  (the text was too long for the search index).
- **SVG attachments** are shown as images instead of being downloaded; they are served with a Content-Security-Policy
  that blocks scripts and external resources.
- **Import**: links to imported documents become mentions of them, with the text of the link (links with an anchor,
  a title or formatted text stay links); in Wiki.js exports, links to pages that are not in the export become plain
  text. The "Referenced by" list of imported documents now includes the links to documents imported after them.

## [0.2.0] - 2026-10-21

### Added

- **Template variables and placeholders**, as in Outline: `{date}`, `{time}`, `{datetime}` and `{author}` in the title
  and text of a template are replaced when a new document is created from it (also through MCP); placeholders
  (`!!text!!`, or the placeholder button in a template's formatting menu) are highlighted in the new document and
  replaced by clicking and typing.
- **Starred collections and ordering of the stars**, as in Outline: collections can be starred too (from the
  collection page or the sidebar menu); in the sidebar the Starred section folds from its heading, starred items open
  like in the tree, are dragged to change the order, and documents or collections dragged there from the tree are
  starred. Documents in the tree have a menu with Star / Unstar.
- **Import from Wiki.js 2**: the export from Administration → Utilities → Export (`pages.json.gz` and assets) or the
  files of the Local File System or Git storage, also disk backups (`.tar.gz` archives are now accepted). Folders
  become collections and nested documents, dates are kept, unpublished pages become drafts, links between pages and
  to assets are rewritten, `{.is-info}` … blockquotes become notices and HTML pages are converted to Markdown.

### Changed

- "Templates" moved from the sidebar to the settings, below "Groups" (`/settings/templates`; the old `/templates`
  address still works).
- "Settings" and "Invite people…" moved from the workspace menu to the bottom of the sidebar, below the trash;
  "Invite people…" is shown only to those who can invite.
- The workspace menu stays short with many workspaces: the list scrolls on its own and, from 8 workspaces, a search
  field filters it by name (Enter opens the first match).
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

[Unreleased]: https://github.com/teopost/getpaper/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/teopost/getpaper/releases/tag/v0.2.0
[0.1.0]: https://github.com/teopost/getpaper/releases/tag/v0.1.0
