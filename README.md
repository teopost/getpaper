# Horsa Codex

Wiki / knowledge base ispirata a [Outline](https://github.com/outline/outline), con backend in **Rust** e
**workspace multipli** inclusi (in Outline sono una funzione a pagamento).

## Avvio

```bash
docker compose up -d --build
```

Apri http://localhost:3000. Il primo accesso propone la creazione del primo account e del primo workspace.

Variabili utili (file `.env` accanto a `docker-compose.yml` oppure in ambiente):

| Variabile | Default | Descrizione |
| --- | --- | --- |
| `URL` | `http://localhost:3000` | URL pubblico, usato nei link di invito e di condivisione |
| `PORT` | `3000` | Porta esposta sull'host |
| `ALLOW_SIGNUP` | `true` | Con `false` ci si registra solo tramite invito (il primo utente può sempre registrarsi) |
| `IMPORT_MAX_MB` | `500` | Dimensione massima di un archivio ZIP da importare |

I dati stanno nei volumi Docker `pgdata` (PostgreSQL) e `uploads` (allegati).

## Funzionalità

- **Workspace multipli**: ogni utente può creare e appartenere a più workspace, completamente isolati
  (membri, ruoli, collezioni, documenti, condivisioni, chiavi API). Selettore in alto a sinistra nella sidebar.
- **Collezioni** con icona e colore, permessi "modifica", "sola lettura" o "privata" con membri espliciti,
  riordino via drag & drop.
- **Documenti annidati** con albero nella sidebar, drag & drop per spostare e riordinare, breadcrumb.
- **Editor WYSIWYG** salvato in Markdown: menu `/`, toolbar di formattazione, scorciatoie Markdown, tabelle,
  checklist, blocchi di codice con evidenziazione, notice (`:::info`, `:::warning`, `:::tip`, `:::success`),
  immagini e allegati con incolla o trascina.
- Salvataggio automatico, bozze, pubblicazione, cronologia revisioni con ripristino, preferiti, archivio,
  cestino con 30 giorni di conservazione, template, commenti con risposte e risoluzione.
- **Ricerca full-text** in PostgreSQL con evidenziazione dei risultati e filtro per collezione.
- **Condivisione pubblica** dei documenti tramite link (`/s/...`).
- **Membri e ruoli** (admin, editor, viewer), inviti tramite link, chiavi API personali.
- Import ed export Markdown, tema chiaro e scuro automatico.

## Accesso con Microsoft (SSO)

Il login usa Microsoft Entra ID (Azure AD) tramite OpenID Connect, con authorization code flow e PKCE.

1. In Entra ID crea una *App registration* di tipo **Web**, con redirect URI `<URL>/auth/microsoft/callback`,
   ad esempio `https://wiki.horsa.it/auth/microsoft/callback`.
2. Crea un *client secret* e imposta le variabili nel `.env`:

| Variabile | Descrizione |
| --- | --- |
| `MICROSOFT_CLIENT_ID` | Application (client) ID |
| `MICROSOFT_CLIENT_SECRET` | Client secret |
| `MICROSOFT_TENANT_ID` | Directory (tenant) ID. Con `organizations` accetta account di qualunque organizzazione |
| `MICROSOFT_ALLOWED_DOMAINS` | Facoltativo: solo questi domini email possono entrare (es. `horsa.it,vidata.it`) |
| `MICROSOFT_AUTO_PROVISION` | `true` (default): crea l'account al primo accesso. Con `false` entra solo chi è invitato |
| `PASSWORD_LOGIN` | `false` per consentire solo l'accesso Microsoft |

Comportamento:

- Gli account sono legati all'identità Microsoft (tenant e object id). Un account esistente con la stessa email
  viene collegato solo se il tenant è specifico o se i domini sono ristretti.
- Al primo accesso l'utente accetta in automatico gli inviti pendenti per la sua email. Entra inoltre nei
  workspace che elencano il suo dominio in **Impostazioni → Details → Allowed domains**, con il ruolo predefinito.
- Funziona anche dalla pagina di invito ("Join with Microsoft") e riporta l'utente alla pagina richiesta prima
  del login.

## Link pubblici

Un documento pubblicato si può rendere visibile fuori dall'autenticazione da **Share → Publish to internet**. Il
link ha la forma `<URL>/s/<token>`, con un token casuale di 24 caratteri (circa 143 bit), quindi non è indovinabile.

- La pagina è in sola lettura, non richiede login e funziona anche con `PASSWORD_LOGIN=false`.
- La pagina è marcata `noindex` e `no-referrer`, così non finisce nei motori di ricerca e non trapela tramite i link in uscita.
- Il link si revoca dallo stesso pannello o da **Impostazioni → Shared Links**.
- La condivisione si può disattivare per l'intero workspace o per una singola collezione.

## Import

Da **Impostazioni → Import** (o dal menu `…` di una collezione → *Import…*) si carica un file `.md`, `.txt`,
`.html` oppure un archivio `.zip` che li contiene. Vanno bene gli export Markdown di Outline, Notion, Obsidian,
Wiki.js, GitBook e BookStack.

- Le cartelle di primo livello diventano collezioni. Se si sceglie una collezione di destinazione, diventano
  documenti al suo interno.
- Le sottocartelle diventano documenti annidati. `Pagina.md` e la cartella `Pagina/` vengono uniti.
  `index.md` o `README.md` dentro una cartella diventa il contenuto della cartella stessa.
- Il titolo viene preso dal front matter `title:`, dal primo `# Titolo` o dal nome del file. I suffissi esadecimali
  di Notion vengono rimossi.
- Immagini e altri file diventano allegati. I link relativi tra documenti, i link agli allegati e i
  `[[wiki link]]` di Obsidian vengono riscritti.
- Una cartella contenitore unica (es. `export/…`) viene ignorata automaticamente.

API: `POST /api/collections.import` (multipart: `file`, `collectionId` opzionale, `publish`).

## Server MCP (per LLM)

Il backend espone un server [MCP](https://modelcontextprotocol.io) su `POST /mcp` (trasporto Streamable HTTP).
L'autenticazione usa una chiave API personale, creata da **Impostazioni → API Keys & MCP**. La chiave vale per
un solo workspace e l'LLM opera con i permessi del suo proprietario.

```bash
claude mcp add --scope user --transport http horsa-codex http://localhost:3000/mcp \
  --header "Authorization: Bearer ol_api_..."
```

Per altri client (Claude Desktop, Cursor, …):

```json
{ "mcpServers": { "horsa-codex": { "type": "http", "url": "http://localhost:3000/mcp",
  "headers": { "Authorization": "Bearer ol_api_..." } } } }
```

Strumenti disponibili:

| Area | Strumenti |
| --- | --- |
| Workspace | `workspace_info` |
| Collezioni | `list_collections`, `get_collection_tree`, `create_collection`, `update_collection` |
| Lettura | `search_documents`, `list_documents`, `list_drafts`, `list_templates`, `get_document`, `list_revisions` |
| Scrittura | `create_document`, `update_document` (anche `append_text`), `move_document`, `archive_document`, `delete_document`, `restore_document` |
| Collaborazione | `list_comments`, `add_comment`, `share_document`, `unshare_document` |

I documenti si indicano per id, urlId o URL completo. Le collezioni si indicano per id, urlId o nome.

## API

API RPC in stile Outline: `POST /api/<metodo>` con corpo JSON, risposta `{ "ok": true, "data": ... }`.
Autenticazione tramite cookie di sessione o `Authorization: Bearer <chiave API>`.

```bash
curl -X POST http://localhost:3000/api/documents.search \
  -H "Authorization: Bearer ol_api_..." -H "Content-Type: application/json" \
  -d '{"query":"onboarding"}'
```

Metodi principali: `auth.*`, `teams.*` (workspace), `users.*`, `invites.*`, `collections.*`, `documents.*`,
`revisions.*`, `stars.*`, `shares.*`, `comments.*`, `attachments.create`, `apiKeys.*`.
L'elenco completo è in `server/src/api/mod.rs`.

## Struttura

```
server/   backend Rust (axum + sqlx + PostgreSQL), migrazioni in server/migrations
web/      frontend React + TipTap (ProseMirror), compilato con Vite e servito dal backend
```

Sviluppo frontend con hot reload: avvia lo stack, poi `cd web && npm install && npm run dev`
(il proxy di Vite inoltra `/api` e `/files` a `localhost:3000`).

## Differenze rispetto a Outline

- Niente collaborazione in tempo reale (Y.js). Il salvataggio è automatico; se un altro utente ha modificato
  il documento nel frattempo, compare un avviso di conflitto con il pulsante per ricaricare.
- Login con email e password. Non ci sono SSO (Google, Slack, OIDC), invio email, gruppi e integrazioni.
- I commenti riguardano l'intero documento, non sono ancorati a una selezione di testo.
