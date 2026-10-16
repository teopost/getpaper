# GetPaper

[English](README.md) · **Italiano**

**Sito del progetto:** https://getpaper.it · Licenza MIT · Immagine Docker: `ghcr.io/teopost/getpaper`

Wiki / knowledge base ispirata a [Outline](https://github.com/outline/outline), con backend in **Rust** e
**workspace multipli** inclusi (in Outline sono una funzione a pagamento).

## Avvio rapido (immagine Docker)

L'immagine pubblicata su GitHub Container Registry (`ghcr.io/teopost/getpaper`, per amd64 e arm64) si avvia senza
scaricare i sorgenti:

```bash
mkdir getpaper && cd getpaper
curl -fsSLO https://raw.githubusercontent.com/teopost/getpaper/main/deploy/docker-compose.yml
curl -fsSL https://raw.githubusercontent.com/teopost/getpaper/main/.env.example -o .env
# imposta almeno URL e POSTGRES_PASSWORD in .env
docker compose up -d
```

Aggiornamento all'ultima versione: `docker compose pull && docker compose up -d`. Per aggiornamenti controllati
imposta in `.env` una versione precisa, ad esempio `GETPAPER_IMAGE=ghcr.io/teopost/getpaper:1.0.0`.

## Avvio dai sorgenti

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
| `SUPERADMIN_EMAILS` | | Account sempre superadmin, separati da virgola (vedi sotto) |
| `DEFAULT_LANGUAGE` | `en` | Lingua dell'interfaccia per chi non l'ha scelta: `en`, `it` |
| `DEFAULT_THEME` | `system` | Aspetto per chi non l'ha scelto: `system`, `light`, `dark` |
| `DEFAULT_CONTENT_WIDTH` | `normal` | Larghezza del testo nei documenti: `normal`, `wide`, `full` |

Ogni utente può sovrascrivere lingua, aspetto e larghezza del testo in **Impostazioni → Profile → Preferenze**.
I valori `DEFAULT_*` valgono anche per le pagine di login e i link pubblici.

I dati stanno nei volumi Docker `pgdata` (PostgreSQL) e `uploads` (allegati).

Comandi più comuni (`make` senza argomenti mostra l'elenco):

| Comando | Cosa fa |
| --- | --- |
| `make start` / `make stop` | Avvia / ferma i container (i dati restano) |
| `make rebuild` | Ricompila l'immagine dai sorgenti e riavvia |
| `make restart` | Riavvia solo l'applicazione |
| `make logs` | Segue il log dell'applicazione |
| `make status` | Stato e salute dei container |
| `make backup` | Copia del database in `/opt/getpaper-backups` (`make backup-clean` elimina quelle vecchie) |
| `make restore FILE=…` | Ripristina una copia del database (chiede conferma) |
| `make psql` / `make shell` | Console del database / shell nel container |
| `make update` | `git pull` e ricompilazione |

## Funzionalità

- **Workspace multipli**: ogni utente può creare e appartenere a più workspace, completamente isolati
  (membri, ruoli, collezioni, documenti, condivisioni, chiavi API). Selettore in alto a sinistra nella sidebar.
- **Collezioni** con icona e colore, permessi "modifica", "sola lettura" o "privata" con membri espliciti,
  riordino via drag & drop.
- **Documenti annidati** con albero nella sidebar, drag & drop per spostare e riordinare, breadcrumb.
- **Spostamento tra workspace**: da *Sposta* si sceglie anche il workspace di destinazione (tra quelli in cui si può
  scrivere). Il documento passa con sottopagine, cronologia, commenti e allegati; i suoi link pubblici vengono revocati.
- **Editor WYSIWYG** salvato in Markdown: menu `/`, toolbar di formattazione, scorciatoie Markdown, tabelle,
  checklist, blocchi di codice con evidenziazione, notice (`:::info`, `:::warning`, `:::tip`, `:::success`),
  immagini e allegati con incolla o trascina.
- **Menzioni** con `@`, come in Outline: persone, gruppi, documenti e collezioni (salvate come
  `@[Nome](mention://<id>/<user|document|collection>/<id>)`); documenti e collezioni sono chip che si aprono con
  un clic, anche in modifica. Chi viene menzionato in un documento pubblicato riceve una notifica nella sidebar
  e un'email (disattivabile nelle preferenze del profilo), se può leggere il documento.
- Salvataggio automatico, bozze, pubblicazione, cronologia revisioni con ripristino, preferiti, archivio,
  cestino con 30 giorni di conservazione, template, commenti con risposte e risoluzione.
- **Ricerca full-text** in PostgreSQL con evidenziazione dei risultati e filtro per collezione.
- **Condivisione pubblica** dei documenti tramite link (`/s/...`).
- **Membri e ruoli** (admin, editor, viewer), inviti tramite link, chiavi API personali.
- **Gruppi**, come in Outline: li creano gli amministratori del workspace, i membri li gestiscono gli amministratori
  del workspace e del gruppo; a un gruppo si può dare accesso in lettura o modifica a una collezione e menzionarlo
  con `@gruppo` avvisa tutti i suoi membri (le menzioni si possono disattivare per gruppo).
- Import ed export Markdown; ogni documento si può scaricare in Markdown, HTML (autonomo, con le immagini
  incorporate), PDF (dalla finestra di stampa del browser) o Word (.docx).
- **Modalità presentazione**, come in Outline: il documento diventa una serie di slide a schermo intero, divise ai
  titoli grandi e medi e ai separatori (frecce, spazio, F per lo schermo intero, Esc per uscire).
- Interfaccia in inglese e italiano, tema chiaro, scuro o automatico.

## Accesso con Microsoft (SSO)

Il login usa Microsoft Entra ID (Azure AD) tramite OpenID Connect, con authorization code flow e PKCE.

1. In Entra ID crea una *App registration* di tipo **Web**, con redirect URI `<URL>/auth/microsoft/callback`,
   ad esempio `https://wiki.example.com/auth/microsoft/callback`.
2. Crea un *client secret* e imposta le variabili nel `.env`:

| Variabile | Descrizione |
| --- | --- |
| `MICROSOFT_CLIENT_ID` | Application (client) ID |
| `MICROSOFT_CLIENT_SECRET` | Client secret |
| `MICROSOFT_TENANT_ID` | Directory (tenant) ID. Con `organizations` accetta account di qualunque organizzazione |
| `MICROSOFT_ALLOWED_DOMAINS` | Facoltativo: solo questi domini email possono entrare (es. `example.com,example.org`) |
| `MICROSOFT_AUTO_PROVISION` | `true` (default): crea l'account al primo accesso. Con `false` entra solo chi è invitato |
| `PASSWORD_LOGIN` | `false` per consentire solo l'accesso Microsoft |
| `MICROSOFT_CALLBACK_PATH` | Percorso del redirect URI: `/auth/microsoft/callback` (predefinito) oppure `/auth/azure.callback`, lo stesso di Outline, per riusare senza modifiche la app registration di un'istanza Outline sullo stesso dominio |

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
- I file HTML vengono convertiti in Markdown.

### Confluence

Si importa l'export **XML** o **HTML** di uno spazio (Cloud: *Impostazioni spazio → Esporta spazio*;
Server/Data Center: *Space tools → Content Tools → Export*). Lo ZIP si carica così com'è; l'importer
riconosce il formato automaticamente.

- **XML** (consigliato, contiene `entities.xml`): vengono importate solo le pagine correnti (non bozze, cestino e
  versioni storiche), con gerarchia, ordine e **date di creazione e modifica** originali. Le menzioni diventano
  `@nome` (dall'account Confluence). `entities.xml` non è soggetto al limite `MAX_UPLOAD_MB` dei singoli file.
- **HTML**: più spazi si possono importare insieme mettendo le cartelle nello stesso ZIP.

- Ogni spazio diventa una collezione (con una collezione di destinazione, le pagine finiscono al suo interno).
- La gerarchia viene ricostruita dai breadcrumb di ogni pagina, l'ordine dall'albero delle pagine in `index.html`.
- Link tra pagine, immagini e allegati vengono riscritti. Gli allegati elencati nella sezione *Attachments* ma non
  mostrati nel testo vengono aggiunti in fondo alla pagina. Icone e fogli di stile dell'export vengono scartati.
- Macro convertite: pannelli info/note/warning/tip → notice, blocchi di codice (con linguaggio), expand, liste di
  attività, tabelle, stati, emoticon, menzioni (come testo). Indice (TOC) e macro dinamiche vengono omessi.
- Link a pagine di altri spazi (non presenti nell'export) restano come testo.
- Non vengono importati: autori (le pagine risultano create da chi importa), commenti, cronologia delle versioni,
  etichette e permessi. L'export PDF non è supportato.

Nel campo **Origine** si sceglie lo strumento di provenienza (Confluence, Outline, Notion, Obsidian, Markdown) per
vedere le istruzioni di esportazione; con *Rilevamento automatico* (predefinito) il formato viene riconosciuto dal
contenuto dello ZIP. Se l'origine scelta non corrisponde all'archivio, l'import viene rifiutato con un messaggio chiaro.
Il risultato indica il formato riconosciuto.

API: `POST /api/collections.import` (multipart: `file`, `source` opzionale tra `auto`, `confluence`, `outline`, `notion`,
`obsidian`, `markdown`, `collectionId` opzionale, `publish`). La risposta contiene `format`
(`confluence-xml`, `confluence-html` o `markdown`).

## Email di invito

Con un server SMTP configurato, gli inviti vengono spediti per email: un messaggio nella lingua di chi invita, con
workspace, ruolo e il pulsante *Accetta l'invito* (con l'accesso Microsoft attivo indica di entrare con l'account
aziendale). Chi ha già un account e viene aggiunto a un workspace riceve un avviso.

- In **Impostazioni → Membri → Invita** la casella *Invia l'invito per email* (attiva di default).
- Negli inviti in sospeso: *Invia email* / *Invia di nuovo*, la data dell'ultimo invio e *Invia email a tutti gli inviti
  in attesa*.
- In **Impostazioni generali → Email** (superadmin): *Verifica connessione* e *Invia un'email di prova* a sé stessi.

| Variabile | Descrizione |
| --- | --- |
| `SMTP_HOST`, `SMTP_PORT` | Server di posta (senza `SMTP_HOST` le email sono disattivate) |
| `SMTP_USERNAME`, `SMTP_PASSWORD` | Credenziali (per SendGrid: `apikey` e la chiave API) |
| `SMTP_FROM_EMAIL`, `SMTP_FROM_NAME` | Mittente; senza `SMTP_FROM_NAME` il nome del mittente è il nome del prodotto |
| `SMTP_SECURE` | `ssl` (porta 465), `starttls` (587) o `none`; se vuoto si deduce dalla porta |

## Superadmin e impostazioni generali

Il **superadmin** gestisce le impostazioni comuni a tutti i workspace dell'installazione, in
**Impostazioni → Impostazioni generali** (la voce compare solo ai superadmin):

- **Aspetto del marchio**: nome del prodotto (se vuoto resta *GetPaper*; è mostrato nella scheda del browser, nella
  pagina di accesso, accanto al logo, nei link pubblici, nelle email e nel documento di benvenuto dei nuovi workspace;
  file di backup e server MCP mantengono il nome tecnico GetPaper), colore dell'header (la barra in alto; il colore del testo è scelto in automatico per il
  contrasto) e colore di accento (pulsanti, link). Valgono anche nelle pagine di accesso e nei link pubblici.
- **Protezione dei dati**: abilita o disabilita i backup dei workspace, l'export Markdown (vault Obsidian e download
  dei singoli documenti) e i link pubblici. Il blocco è applicato dal server; disattivando i link pubblici quelli
  esistenti smettono di funzionare finché non vengono riattivati.
- **Valori predefiniti per gli utenti**: lingua, aspetto e larghezza dei documenti. Hanno la precedenza sulle
  variabili `DEFAULT_*`, che restano il valore di partenza.
- **Superadmin**: elenco, aggiunta per email (l'account deve esistere) e rimozione (ne resta sempre almeno uno).

Il primo account creato è superadmin. I superadmin hanno il badge *Superadmin* nell'elenco **Membri** e sono elencati
in **Impostazioni generali → Superadmin**. Con `SUPERADMIN_EMAILS` (separati da virgola) si possono indicare account che
sono sempre superadmin, ad esempio per recuperare l'accesso. Essere superadmin non dà accesso ai workspace di cui non
si è membri.

## Export

Da **Impostazioni → Esporta**:

- **Backup per GetPaper** (solo amministratori): ZIP con `getpaper-backup.json` e la cartella `attachments/`.
  Contiene collezioni (icona, colore, permessi, membri delle collezioni private), documenti con gerarchia, bozze,
  modelli e archiviati, date, cronologia delle revisioni, commenti e allegati. Si ripristina in un'altra istanza da
  **Impostazioni → Import** (origine *Backup GetPaper* o rilevamento automatico) nel workspace corrente:
  - gli autori vengono abbinati per email ai membri del workspace di destinazione; il resto viene attribuito a chi
    ripristina;
  - id e link restano gli stessi se liberi; se sono già usati (es. ripristino nella stessa istanza) vengono rigenerati
    e i link interni riscritti;
  - non sono inclusi: utenti e ruoli, impostazioni del workspace, link pubblici, preferiti, chiavi API.
- **Vault Obsidian**: i documenti pubblicati leggibili dall'utente come file Markdown. Una cartella per collezione,
  documenti annidati come `Pagina.md` + `Pagina/`, link interni come `[[wiki link]]`, allegati in `_attachments/`,
  notice come callout (`> [!info]`), date e titolo originale nel front matter. Reimportando il vault (origine
  *Obsidian*) callout e titoli vengono riconvertiti.

API: `POST /api/workspace.export` con `{"format": "backup" | "obsidian"}`, risponde con lo ZIP.

## Server MCP (per LLM)

Il backend espone un server [MCP](https://modelcontextprotocol.io) su `POST /mcp` (trasporto Streamable HTTP).
L'autenticazione usa una chiave API personale, creata da **Impostazioni → API Keys & MCP**. Alla creazione si sceglie:

- **Accesso**: *sola lettura* (l'LLM cerca e legge) oppure *lettura e scrittura*;
- **Workspace**: solo quello corrente, alcuni, oppure tutti quelli dell'utente (anche futuri);
- **Collezioni**: tutte oppure solo alcune (in quel caso l'LLM non vede altro, nemmeno le bozze senza collezione,
  e non può creare collezioni);
- **Scadenza**: nessuna, 30, 90, 180 giorni o 1 anno.

Il permesso effettivo non supera mai quello dell'utente; con una chiave non si possono amministrare i workspace
(membri, impostazioni), modificare le impostazioni generali né creare altre chiavi. Una chiave scaduta viene rifiutata.

Una chiave valida in più workspace richiede una sola configurazione nel client: lo strumento `list_workspaces` elenca i
workspace della chiave, ogni strumento accetta il parametro facoltativo `workspace` (nome o id; senza, si usa il
workspace predefinito della chiave) e `search_documents` cerca in tutti con `all_workspaces: true`. Nelle API REST il
workspace si sceglie con l'intestazione `X-Workspace: <id>`.

```bash
claude mcp add --scope user --transport http getpaper http://localhost:3000/mcp \
  --header "Authorization: Bearer ol_api_..."
```

Per altri client (Claude Desktop, Cursor, …):

```json
{ "mcpServers": { "getpaper": { "type": "http", "url": "http://localhost:3000/mcp",
  "headers": { "Authorization": "Bearer ol_api_..." } } } }
```

Strumenti disponibili:

| Area | Strumenti |
| --- | --- |
| Workspace | `list_workspaces`, `workspace_info` |
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
- Accesso con email e password oppure Microsoft Entra ID. Non ci sono altri SSO (Google, Slack, OIDC generico),
  gruppi e integrazioni.
- I commenti riguardano l'intero documento, non sono ancorati a una selezione di testo.

## Licenza

GetPaper è distribuito con licenza [MIT](LICENSE).
