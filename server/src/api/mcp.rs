//! Model Context Protocol server (Streamable HTTP transport, JSON responses).
//!
//! Endpoint: `POST /mcp`, authenticated with a personal API key:
//! `Authorization: Bearer ol_api_…`. The key is scoped to one workspace, so the LLM
//! acts with the permissions of the key owner inside that workspace.

use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    api::{collections, comments, documents, revisions, session, shares},
    auth::Auth,
    error::{ApiError, ApiResult},
    util::slugify,
    AppState,
};

const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "GetPaper is a team wiki (Outline compatible). Content is organised in collections; \
documents can be nested and are written in Markdown (GitHub flavoured, plus notice blocks `:::info … :::`, \
`:::warning`, `:::tip`, `:::success`). Start with list_collections or search_documents. Documents can be referenced \
by id, urlId or full URL; collections by id, urlId or name. New documents are published by default; pass \
publish=false to keep a private draft. Prefer update_document with append_text to add content without rewriting. \
The API key may be valid in several workspaces: list_workspaces shows them; every tool takes an optional `workspace` \
(name or id, default: the key's default workspace) and search_documents can search all of them with all_workspaces=true.";

// ---------------------------------------------------------------------------
// transport
// ---------------------------------------------------------------------------

fn rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub async fn get_not_allowed() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST")], "This MCP endpoint only supports POST").into_response()
}

pub async fn handle(State(st): State<AppState>, auth: Result<Auth, ApiError>, _headers: HeaderMap, body: Bytes) -> Response {
    let auth = match auth {
        Ok(a) => a,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer realm=\"getpaper\"")],
                Json(rpc_error(Value::Null, -32001, "Authentication required: send an API key as 'Authorization: Bearer ol_api_…'")),
            )
                .into_response()
        }
    };
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(rpc_error(Value::Null, -32700, format!("Parse error: {e}")))).into_response(),
    };
    match msg {
        Value::Array(items) => {
            let mut out = vec![];
            for m in items {
                if let Some(r) = dispatch(&st, &auth, m).await {
                    out.push(r);
                }
            }
            if out.is_empty() {
                StatusCode::ACCEPTED.into_response()
            } else {
                Json(Value::Array(out)).into_response()
            }
        }
        m => match dispatch(&st, &auth, m).await {
            Some(r) => Json(r).into_response(),
            None => StatusCode::ACCEPTED.into_response(),
        },
    }
}

async fn dispatch(st: &AppState, auth: &Auth, msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    // notifications and responses carry no id: nothing to answer
    let id = id?;
    Some(match method.as_str() {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("");
            let version = if SUPPORTED_VERSIONS.contains(&requested) { requested } else { SUPPORTED_VERSIONS[0] };
            rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "getpaper", "title": "GetPaper wiki", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, json!({ "tools": tool_definitions() })),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if !tool_definitions().iter().any(|t| t["name"] == name) {
                return Some(rpc_error(id, -32602, format!("Unknown tool: {name}")));
            }
            // run the tool in the requested workspace, if the key allows it
            let scoped = match arg(&args, "workspace").filter(|w| !w.trim().is_empty()) {
                Some(w) => resolve_workspace(st, auth, &w).await,
                None => Ok(auth.clone()),
            };
            let (text, is_error) = match scoped {
                Ok(a) => match call_tool(st, &a, name, args).await {
                    Ok(t) => (t, false),
                    Err(e) => (e, true),
                },
                Err(e) => (e, true),
            };
            rpc_result(id, json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
        }
        "resources/list" => rpc_result(id, json!({ "resources": [] })),
        "prompts/list" => rpc_result(id, json!({ "prompts": [] })),
        _ => rpc_error(id, -32601, format!("Method not found: {method}")),
    })
}

// ---------------------------------------------------------------------------
// tool definitions
// ---------------------------------------------------------------------------

fn tool(name: &str, title: &str, description: &str, properties: Value, required: &[&str], read_only: bool) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
        "annotations": { "readOnlyHint": read_only, "destructiveHint": false },
    })
}

/// Every tool takes an optional `workspace`; search_documents can also search all of them.
fn tool_definitions() -> Vec<Value> {
    let mut tools = base_tool_definitions();
    for t in tools.iter_mut() {
        let name = t["name"].as_str().unwrap_or("").to_string();
        if name == "list_workspaces" {
            continue;
        }
        let props = &mut t["inputSchema"]["properties"];
        props["workspace"] = json!({
            "type": "string",
            "description": "Workspace name or id (see list_workspaces). Default: the API key's default workspace."
        });
        if name == "search_documents" {
            props["all_workspaces"] = json!({ "type": "boolean", "description": "Search every workspace the key is valid in" });
        }
    }
    tools
}

fn base_tool_definitions() -> Vec<Value> {
    let doc_id = json!({ "type": "string", "description": "Document id, urlId or URL" });
    let coll = json!({ "type": "string", "description": "Collection id, urlId or name" });
    vec![
        tool(
            "list_workspaces",
            "List workspaces",
            "List the workspaces this API key can be used in, with the user's effective role and which one is the default.",
            json!({}),
            &[],
            true,
        ),
        tool("workspace_info", "Workspace info", "Return the current workspace, the authenticated user and their role.", json!({}), &[], true),
        tool("list_collections", "List collections", "List the collections the user can access in the workspace.", json!({}), &[], true),
        tool(
            "get_collection_tree",
            "Collection structure",
            "Return the nested tree of published documents in a collection (titles and ids).",
            json!({ "collection": coll }),
            &["collection"],
            true,
        ),
        tool(
            "create_collection",
            "Create collection",
            "Create a new collection.",
            json!({
                "name": { "type": "string" },
                "description": { "type": "string", "description": "Markdown description" },
                "icon": { "type": "string", "description": "One of: collection, book, home, star, code, briefcase, cloud, coffee, database, flag, globe, heart, lightbulb, rocket, settings, users, tool, calendar, camera, leaf, palette, shield, zap, academic, music, map, package, lock" },
                "color": { "type": "string", "description": "Hex color, e.g. #0366d6" },
                "permission": { "type": "string", "enum": ["read_write", "read", "private"], "description": "Default access for workspace members (default read_write)" }
            }),
            &["name"],
            false,
        ),
        tool(
            "update_collection",
            "Update collection",
            "Rename a collection or change its description, icon, color or permission.",
            json!({
                "collection": coll,
                "name": { "type": "string" },
                "description": { "type": "string" },
                "icon": { "type": "string" },
                "color": { "type": "string" },
                "permission": { "type": "string", "enum": ["read_write", "read", "private"] }
            }),
            &["collection"],
            false,
        ),
        tool(
            "search_documents",
            "Search documents",
            "Full-text search across titles and content. Supports quotes for phrases and -word to exclude.",
            json!({
                "query": { "type": "string" },
                "collection": coll,
                "limit": { "type": "integer", "minimum": 1, "maximum": 50 }
            }),
            &["query"],
            true,
        ),
        tool(
            "list_documents",
            "List documents",
            "List published documents, most recently updated first, optionally filtered by collection or parent document.",
            json!({
                "collection": coll,
                "parent_document": doc_id,
                "sort": { "type": "string", "enum": ["updatedAt", "createdAt", "publishedAt", "title"] },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
            }),
            &[],
            true,
        ),
        tool("list_drafts", "List drafts", "List the user's unpublished drafts.", json!({}), &[], true),
        tool("list_templates", "List templates", "List the document templates of the workspace.", json!({}), &[], true),
        tool(
            "get_document",
            "Read document",
            "Return a document as Markdown together with its metadata, location and nested documents.",
            json!({ "id": doc_id }),
            &["id"],
            true,
        ),
        tool(
            "create_document",
            "Create document",
            "Create a document in a collection or nested under another document. Published by default.",
            json!({
                "title": { "type": "string" },
                "text": { "type": "string", "description": "Markdown body (do not repeat the title as a heading)" },
                "collection": coll,
                "parent_document": { "type": "string", "description": "Parent document id/urlId/URL to nest under" },
                "emoji": { "type": "string", "description": "Optional emoji icon" },
                "publish": { "type": "boolean", "description": "Default true; false creates a private draft" },
                "template": { "type": "string", "description": "Optional template id to start from" }
            }),
            &["title"],
            false,
        ),
        tool(
            "update_document",
            "Update document",
            "Update a document. `text` replaces the whole body; `append_text` appends Markdown at the end. Set publish=true to publish a draft.",
            json!({
                "id": doc_id,
                "title": { "type": "string" },
                "text": { "type": "string" },
                "append_text": { "type": "string" },
                "emoji": { "type": "string", "description": "Emoji, or empty string to remove" },
                "publish": { "type": "boolean" },
                "collection": { "type": "string", "description": "Collection to publish a draft into" }
            }),
            &["id"],
            false,
        ),
        tool(
            "move_document",
            "Move document",
            "Move a document (with its nested documents) to another collection or under another document.",
            json!({
                "id": doc_id,
                "collection": coll,
                "parent_document": { "type": "string" },
                "index": { "type": "integer", "minimum": 0, "description": "Position among siblings" }
            }),
            &["id"],
            false,
        ),
        tool("archive_document", "Archive document", "Archive a document and its nested documents.", json!({ "id": doc_id }), &["id"], false),
        json!({
            "name": "delete_document",
            "title": "Delete document",
            "description": "Move a document and its nested documents to the trash (recoverable for 30 days).",
            "inputSchema": { "type": "object", "properties": { "id": doc_id }, "required": ["id"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": true },
        }),
        tool(
            "restore_document",
            "Restore document",
            "Restore an archived or deleted document, or roll it back to a revision.",
            json!({ "id": doc_id, "revision_id": { "type": "string" }, "collection": coll }),
            &["id"],
            false,
        ),
        tool("list_revisions", "List revisions", "List the revision history of a document.", json!({ "id": doc_id }), &["id"], true),
        tool("list_comments", "List comments", "List the comments of a document.", json!({ "id": doc_id }), &["id"], true),
        tool(
            "add_comment",
            "Add comment",
            "Add a comment (or a reply) to a document.",
            json!({ "id": doc_id, "text": { "type": "string" }, "reply_to": { "type": "string", "description": "Comment id to reply to" } }),
            &["id", "text"],
            false,
        ),
        tool("share_document", "Share publicly", "Create (or return) a public share link for a published document.", json!({ "id": doc_id }), &["id"], false),
        tool("unshare_document", "Revoke public link", "Revoke the public share link of a document.", json!({ "id": doc_id }), &["id"], false),
    ]
}

// ---------------------------------------------------------------------------
// tool implementations
// ---------------------------------------------------------------------------

fn req<T: DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {e}"))
}

fn data(r: ApiResult<Json<Value>>) -> Result<Value, String> {
    r.map(|j| j.0["data"].clone()).map_err(|e| e.message)
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

fn arg(args: &Value, k: &str) -> Option<String> {
    args.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

/// Accepts ids, urlIds, "slug-urlId" and full URLs.
fn doc_ref(v: &str) -> String {
    let v = v.trim().trim_end_matches('/');
    let v = v.split(['?', '#']).next().unwrap_or(v);
    v.rsplit('/').next().unwrap_or(v).to_string()
}

fn doc_url(st: &AppState, d: &Value) -> String {
    format!("{}/doc/{}-{}", st.cfg.url.trim_end_matches('/'), slugify(s(d, "title")), s(d, "urlId"))
}

async fn resolve_collection(st: &AppState, auth: &Auth, name: &str) -> Result<String, String> {
    let r = doc_ref(name);
    let cols = data(collections::list(State(st.clone()), auth.clone()).await)?;
    let found = cols.as_array().into_iter().flatten().find(|c| {
        s(c, "id") == r || s(c, "urlId") == r || r.ends_with(&format!("-{}", s(c, "urlId"))) || s(c, "name").eq_ignore_ascii_case(name.trim())
    });
    match found {
        Some(c) => Ok(s(c, "id").to_string()),
        None => {
            let names: Vec<&str> = cols.as_array().into_iter().flatten().map(|c| s(c, "name")).collect();
            Err(format!("Collection \"{name}\" not found. Available collections: {}", names.join(", ")))
        }
    }
}

async fn opt_collection(st: &AppState, auth: &Auth, args: &Value, key: &str) -> Result<Option<String>, String> {
    match arg(args, key) {
        Some(c) => Ok(Some(resolve_collection(st, auth, &c).await?)),
        None => Ok(None),
    }
}

fn doc_line(st: &AppState, d: &Value) -> String {
    let emoji = d.get("emoji").and_then(|e| e.as_str()).map(|e| format!("{e} ")).unwrap_or_default();
    let draft = if d.get("publishedAt").map(|p| p.is_null()).unwrap_or(false) { " [draft]" } else { "" };
    format!(
        "- {emoji}{}{draft} - id: {} - updated {} by {} - {}",
        if s(d, "title").is_empty() { "Untitled" } else { s(d, "title") },
        s(d, "id"),
        &s(d, "updatedAt")[..s(d, "updatedAt").len().min(16)],
        s(d, "updatedByName"),
        doc_url(st, d)
    )
}

fn doc_list(st: &AppState, docs: &Value, empty: &str) -> String {
    let lines: Vec<String> = docs.as_array().into_iter().flatten().map(|d| doc_line(st, d)).collect();
    if lines.is_empty() {
        empty.to_string()
    } else {
        lines.join("\n")
    }
}

fn tree_lines(nodes: &Value, depth: usize, out: &mut Vec<String>) {
    for n in nodes.as_array().into_iter().flatten() {
        let emoji = n.get("emoji").and_then(|e| e.as_str()).map(|e| format!("{e} ")).unwrap_or_default();
        out.push(format!("{}- {emoji}{} (id: {})", "  ".repeat(depth), s(n, "title"), s(n, "id")));
        tree_lines(&n["children"], depth + 1, out);
    }
}

fn summary(st: &AppState, verb: &str, d: &Value) -> String {
    format!(
        "{verb} \"{}\"{}\nid: {}\nurl: {}",
        s(d, "title"),
        if d.get("publishedAt").map(|p| p.is_null()).unwrap_or(false) { " (draft)" } else { "" },
        s(d, "id"),
        doc_url(st, d)
    )
}

/// Workspaces usable by this request: the key's, or every membership for a browser session.
async fn usable_workspaces(st: &AppState, auth: &Auth) -> Result<Vec<(Uuid, String, String)>, String> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT t.id, t.name, m.role FROM team_memberships m JOIN teams t ON t.id = m.team_id WHERE m.user_id = $1 ORDER BY t.name",
    )
    .bind(auth.user.id)
    .fetch_all(&st.db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(match &auth.key {
        Some(k) => {
            let mut v: Vec<_> = rows.into_iter().filter(|r| k.teams.contains(&r.0)).collect();
            v.sort_by_key(|r| k.teams.iter().position(|t| *t == r.0));
            v
        }
        None => rows,
    })
}

/// Workspace by id or (case-insensitive) name among those usable by the key.
async fn resolve_workspace(st: &AppState, auth: &Auth, wanted: &str) -> Result<Auth, String> {
    let all = usable_workspaces(st, auth).await?;
    let w = wanted.trim();
    let found = all
        .iter()
        .find(|t| t.0.to_string() == w)
        .or_else(|| all.iter().find(|t| t.1.eq_ignore_ascii_case(w)))
        .ok_or_else(|| {
            let names: Vec<&str> = all.iter().map(|t| t.1.as_str()).collect();
            format!("Workspace \"{w}\" not available for this key. Available: {}", names.join(", "))
        })?;
    auth.for_team(&st.db, found.0).await.map_err(|e| e.message)
}

async fn call_tool(st: &AppState, auth: &Auth, name: &str, args: Value) -> Result<String, String> {
    let st_ = || State(st.clone());
    let au = || auth.clone();
    match name {
        "list_workspaces" => {
            let all = usable_workspaces(st, auth).await?;
            let default = auth.key.as_ref().and_then(|k| k.teams.first().copied()).or(auth.team_id);
            let collections = match auth.key.as_ref().and_then(|k| k.collections.as_ref()) {
                Some(c) => format!("\nThis key is limited to {} collection(s).", c.len()),
                None => String::new(),
            };
            let access = match &auth.key {
                Some(k) if !k.write => "\nThis key is read-only.",
                _ => "",
            };
            let lines: Vec<String> = all
                .iter()
                .map(|(id, name, role)| {
                    let role = match &auth.key {
                        Some(k) => crate::auth::key_role(role, k),
                        None => role.clone(),
                    };
                    format!("- {name} - id: {id} - role: {role}{}", if Some(*id) == default { " - default" } else { "" })
                })
                .collect();
            Ok(format!("{}{collections}{access}", lines.join("\n")))
        }
        "workspace_info" => {
            let i = data(session::info(st_(), au()).await)?;
            Ok(format!(
                "Workspace: {} (id: {})\nUser: {} <{}>\nRole: {}\nPublic URL: {}",
                s(&i["team"], "name"),
                s(&i["team"], "id"),
                s(&i["user"], "name"),
                s(&i["user"], "email"),
                i["role"].as_str().unwrap_or(""),
                st.cfg.url
            ))
        }
        "list_collections" => {
            let cols = data(collections::list(st_(), au()).await)?;
            let base = st.cfg.url.trim_end_matches('/');
            let lines: Vec<String> = cols
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| {
                    format!(
                        "- {} - id: {} - access: {} - permission: {} - {base}/collection/{}-{}{}",
                        s(c, "name"),
                        s(c, "id"),
                        s(c, "access"),
                        c["permission"].as_str().unwrap_or("private"),
                        slugify(s(c, "name")),
                        s(c, "urlId"),
                        if s(c, "description").is_empty() { String::new() } else { format!("\n  {}", s(c, "description").replace('\n', " ")) }
                    )
                })
                .collect();
            Ok(if lines.is_empty() { "No collections.".into() } else { lines.join("\n") })
        }
        "get_collection_tree" => {
            let cid = resolve_collection(st, auth, &arg(&args, "collection").unwrap_or_default()).await?;
            let tree = data(collections::documents(st_(), au(), Json(req(json!({ "id": cid }))?)).await)?;
            let mut out = vec![];
            tree_lines(&tree, 0, &mut out);
            Ok(if out.is_empty() { "The collection is empty.".into() } else { out.join("\n") })
        }
        "create_collection" => {
            let c = data(
                collections::create(
                    st_(),
                    au(),
                    Json(req(json!({
                        "name": arg(&args, "name"),
                        "description": arg(&args, "description"),
                        "icon": arg(&args, "icon"),
                        "color": arg(&args, "color"),
                        "permission": arg(&args, "permission"),
                    }))?),
                )
                .await,
            )?;
            Ok(format!("Created collection \"{}\" (id: {})", s(&c, "name"), s(&c, "id")))
        }
        "update_collection" => {
            let cid = resolve_collection(st, auth, &arg(&args, "collection").unwrap_or_default()).await?;
            let mut body = json!({ "id": cid });
            for k in ["name", "description", "icon", "color", "permission"] {
                if let Some(v) = args.get(k).and_then(|v| v.as_str()) {
                    body[k] = json!(v);
                }
            }
            let c = data(collections::update(st_(), au(), Json(req(body)?)).await)?;
            Ok(format!("Updated collection \"{}\"", s(&c, "name")))
        }
        "search_documents" => {
            let cid = opt_collection(st, auth, &args, "collection").await?;
            let res = data(
                documents::search(
                    st_(),
                    au(),
                    Json(req(json!({
                        "query": arg(&args, "query").unwrap_or_default(),
                        "collectionId": cid,
                        "allWorkspaces": args.get("all_workspaces").and_then(|v| v.as_bool()).unwrap_or(false),
                        "limit": args.get("limit").and_then(|l| l.as_i64()).unwrap_or(10),
                    }))?),
                )
                .await,
            )?;
            let lines: Vec<String> = res
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| {
                    let ctx = s(r, "context").replace('⟪', "**").replace('⟫', "**").replace('\n', " ");
                    format!("{}\n  {}", doc_line(st, &r["document"]), ctx.trim())
                })
                .collect();
            Ok(if lines.is_empty() { "No documents found.".into() } else { lines.join("\n") })
        }
        "list_documents" => {
            let cid = opt_collection(st, auth, &args, "collection").await?;
            let parent = match arg(&args, "parent_document") {
                Some(p) => Some(data(documents::info(st_(), Some(au()), Json(req(json!({ "id": doc_ref(&p) }))?)).await)?["document"]["id"].clone()),
                None => None,
            };
            let mut body = json!({
                "collectionId": cid,
                "parentDocumentId": parent,
                "limit": args.get("limit").and_then(|l| l.as_i64()).unwrap_or(25),
            });
            if let Some(sort) = arg(&args, "sort") {
                body["sort"] = json!(sort);
                if sort == "title" {
                    body["direction"] = json!("ASC");
                }
            }
            let docs = data(documents::list(st_(), au(), Json(req(body)?)).await)?;
            Ok(doc_list(st, &docs, "No documents."))
        }
        "list_drafts" => {
            let docs = data(documents::drafts(st_(), au(), Json(req(json!({ "limit": 100 }))?)).await)?;
            Ok(doc_list(st, &docs, "No drafts."))
        }
        "list_templates" => {
            let docs = data(documents::templates(st_(), au()).await)?;
            Ok(doc_list(st, &docs, "No templates."))
        }
        "get_document" => {
            let id = doc_ref(&arg(&args, "id").unwrap_or_default());
            let i = data(documents::info(st_(), Some(au()), Json(req(json!({ "id": id }))?)).await)?;
            let d = &i["document"];
            let mut meta = vec![
                format!("id: {}", s(d, "id")),
                format!("url: {}", doc_url(st, d)),
                format!("access: {}", s(&i, "access")),
                format!("status: {}", if d["publishedAt"].is_null() { "draft" } else { "published" }),
                format!("updated: {} by {}", s(d, "updatedAt"), s(d, "updatedByName")),
                format!("created by: {}", s(d, "createdByName")),
            ];
            if !i["collection"].is_null() {
                meta.push(format!("collection: {} (id: {})", s(&i["collection"], "name"), s(&i["collection"], "id")));
            }
            let path: Vec<String> = i["path"].as_array().into_iter().flatten().map(|p| format!("{} ({})", s(p, "title"), s(p, "id"))).collect();
            if !path.is_empty() {
                meta.push(format!("parents: {}", path.join(" > ")));
            }
            let children: Vec<String> = i["children"].as_array().into_iter().flatten().map(|p| format!("{} ({})", s(p, "title"), s(p, "id"))).collect();
            if !children.is_empty() {
                meta.push(format!("nested documents: {}", children.join(", ")));
            }
            if !d["archivedAt"].is_null() {
                meta.push("archived: yes".into());
            }
            if !d["deletedAt"].is_null() {
                meta.push("in trash: yes".into());
            }
            let emoji = d.get("emoji").and_then(|e| e.as_str()).map(|e| format!("{e} ")).unwrap_or_default();
            Ok(format!("# {emoji}{}\n\n{}\n\n---\n\n{}", s(d, "title"), meta.join("\n"), s(d, "text")))
        }
        "create_document" => {
            let cid = opt_collection(st, auth, &args, "collection").await?;
            let parent = arg(&args, "parent_document").map(|p| doc_ref(&p));
            let publish = args.get("publish").and_then(|p| p.as_bool()).unwrap_or(true);
            if publish && cid.is_none() && parent.is_none() {
                return Err("Specify `collection` or `parent_document` to publish the document (or set publish=false for a draft). Use list_collections to see the options.".into());
            }
            let d = data(
                documents::create(
                    st_(),
                    au(),
                    Json(req(json!({
                        "title": arg(&args, "title").unwrap_or_default(),
                        "text": args.get("text").and_then(|t| t.as_str()).unwrap_or(""),
                        "emoji": arg(&args, "emoji"),
                        "collectionId": cid,
                        "parentDocumentId": parent,
                        "publish": publish,
                        "templateId": arg(&args, "template").map(|t| doc_ref(&t)),
                    }))?),
                )
                .await,
            )?;
            Ok(summary(st, "Created", &d))
        }
        "update_document" => {
            let id = doc_ref(&arg(&args, "id").unwrap_or_default());
            let mut body = json!({ "id": id });
            if let Some(t) = args.get("title").and_then(|t| t.as_str()) {
                body["title"] = json!(t);
            }
            if let Some(t) = args.get("text").and_then(|t| t.as_str()) {
                body["text"] = json!(t);
            }
            if let Some(extra) = args.get("append_text").and_then(|t| t.as_str()) {
                let current = match body.get("text").and_then(|t| t.as_str()) {
                    Some(t) => t.to_string(),
                    None => {
                        let i = data(documents::info(st_(), Some(au()), Json(req(json!({ "id": id }))?)).await)?;
                        s(&i["document"], "text").to_string()
                    }
                };
                let sep = if current.trim().is_empty() { "" } else { "\n\n" };
                body["text"] = json!(format!("{}{sep}{}", current.trim_end(), extra));
            }
            if let Some(e) = args.get("emoji").and_then(|t| t.as_str()) {
                body["emoji"] = json!(e);
            }
            if let Some(p) = args.get("publish").and_then(|t| t.as_bool()) {
                body["publish"] = json!(p);
            }
            if let Some(c) = opt_collection(st, auth, &args, "collection").await? {
                body["collectionId"] = json!(c);
            }
            let d = data(documents::update(st_(), au(), Json(req(body)?)).await)?;
            Ok(summary(st, "Updated", &d))
        }
        "move_document" => {
            let id = doc_ref(&arg(&args, "id").unwrap_or_default());
            let cid = opt_collection(st, auth, &args, "collection").await?;
            let d = data(
                documents::move_doc(
                    st_(),
                    au(),
                    Json(req(json!({
                        "id": id,
                        "collectionId": cid,
                        "parentDocumentId": arg(&args, "parent_document").map(|p| doc_ref(&p)),
                        "index": args.get("index").and_then(|i| i.as_u64()),
                    }))?),
                )
                .await,
            )?;
            Ok(summary(st, "Moved", &d))
        }
        "archive_document" => {
            let d = data(documents::archive(st_(), au(), Json(req(json!({ "id": doc_ref(&arg(&args, "id").unwrap_or_default()) }))?)).await)?;
            Ok(summary(st, "Archived", &d))
        }
        "delete_document" => {
            data(documents::delete(st_(), au(), Json(req(json!({ "id": doc_ref(&arg(&args, "id").unwrap_or_default()) }))?)).await)?;
            Ok("Document moved to the trash. It can be restored with restore_document within 30 days.".into())
        }
        "restore_document" => {
            let cid = opt_collection(st, auth, &args, "collection").await?;
            let d = data(
                documents::restore(
                    st_(),
                    au(),
                    Json(req(json!({
                        "id": doc_ref(&arg(&args, "id").unwrap_or_default()),
                        "revisionId": arg(&args, "revision_id"),
                        "collectionId": cid,
                    }))?),
                )
                .await,
            )?;
            Ok(summary(st, "Restored", &d))
        }
        "list_revisions" => {
            let r = data(revisions::list(st_(), au(), Json(req(json!({ "documentId": doc_ref(&arg(&args, "id").unwrap_or_default()) }))?)).await)?;
            let lines: Vec<String> = r
                .as_array()
                .into_iter()
                .flatten()
                .map(|x| format!("- {} - {} - \"{}\" - revision_id: {}", s(x, "createdAt"), s(x, "userName"), s(x, "title"), s(x, "id")))
                .collect();
            Ok(if lines.is_empty() { "No revisions.".into() } else { lines.join("\n") })
        }
        "list_comments" => {
            let r = data(comments::list(st_(), au(), Json(req(json!({ "documentId": doc_ref(&arg(&args, "id").unwrap_or_default()) }))?)).await)?;
            let lines: Vec<String> = r
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| {
                    format!(
                        "- [{}] {}{} ({}){}: {}",
                        s(c, "id"),
                        if c["parentCommentId"].is_null() { "" } else { "↳ " },
                        s(c, "createdByName"),
                        s(c, "createdAt"),
                        if c["resolvedAt"].is_null() { "" } else { " [resolved]" },
                        s(c, "text")
                    )
                })
                .collect();
            Ok(if lines.is_empty() { "No comments.".into() } else { lines.join("\n") })
        }
        "add_comment" => {
            let c = data(
                comments::create(
                    st_(),
                    au(),
                    Json(req(json!({
                        "documentId": doc_ref(&arg(&args, "id").unwrap_or_default()),
                        "text": arg(&args, "text").unwrap_or_default(),
                        "parentCommentId": arg(&args, "reply_to"),
                    }))?),
                )
                .await,
            )?;
            Ok(format!("Comment added (id: {})", s(&c, "id")))
        }
        "share_document" => {
            let id = doc_ref(&arg(&args, "id").unwrap_or_default());
            let i = data(documents::info(st_(), Some(au()), Json(req(json!({ "id": id }))?)).await)?;
            let sh = data(shares::create(st_(), au(), Json(req(json!({ "documentId": s(&i["document"], "id") }))?)).await)?;
            Ok(format!("Public link: {}", s(&sh, "url")))
        }
        "unshare_document" => {
            let id = doc_ref(&arg(&args, "id").unwrap_or_default());
            let i = data(documents::info(st_(), Some(au()), Json(req(json!({ "id": id }))?)).await)?;
            let sh = data(shares::info(st_(), au(), Json(req(json!({ "documentId": s(&i["document"], "id") }))?)).await)?;
            if sh.is_null() {
                return Ok("The document is not shared publicly.".into());
            }
            data(shares::revoke(st_(), au(), Json(req(json!({ "id": s(&sh, "id") }))?)).await)?;
            Ok("Public link revoked.".into())
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}
