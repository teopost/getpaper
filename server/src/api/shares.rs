use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    audit::{self, Target},
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    models::Access,
    perm::load_doc,
    util::random_string,
    AppState,
};

#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct ShareRow {
    id: Uuid,
    url_id: String,
    document_id: Uuid,
    document_title: String,
    document_url_id: String,
    created_by: Uuid,
    created_by_name: String,
    views: i32,
    last_accessed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    /// the nested documents can be read through the link too
    include_child_documents: bool,
    /// search engines may index the pages of the link
    allow_indexing: bool,
    show_last_updated: bool,
    /// "public": anyone with the link; "organization": members of the workspace and signed-in people
    /// with a verified email of one of the organization domains of the workspace
    audience: String,
}

const SHARE_SELECT: &str = "SELECT s.id, s.url_id, s.document_id, d.title AS document_title, d.url_id AS document_url_id, \
    s.created_by, u.name AS created_by_name, s.views, s.last_accessed_at, s.created_at, \
    s.include_children AS include_child_documents, s.allow_indexing, s.show_last_updated, s.audience \
    FROM shares s JOIN documents d ON d.id = s.document_id JOIN users u ON u.id = s.created_by";

fn with_url(st: &AppState, s: ShareRow) -> Value {
    let url = format!("{}/s/{}", st.cfg.url.trim_end_matches('/'), s.url_id);
    let mut v = serde_json::to_value(&s).unwrap();
    v["url"] = json!(url);
    v
}

/// A link with the people outside the workspace who read it, for links for the organization
async fn share_json(st: &AppState, s: ShareRow) -> ApiResult<Value> {
    let organization = s.audience == "organization";
    let (doc, created) = (s.document_id, s.created_at);
    let mut v = with_url(st, s);
    if organization {
        v["readers"] = json!(organization_readers(st, doc, created).await?);
    }
    Ok(v)
}

/// A person outside the workspace who opened a link for the organization
#[derive(Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct LinkReader {
    id: Uuid,
    name: String,
    color: String,
    last_viewed_at: DateTime<Utc>,
}

/// Who opened the document through a link for the organization and is not a member of its workspace
/// (most recent first). Members of the workspace are listed in Share → Read by.
async fn organization_readers(st: &AppState, doc: Uuid, since: DateTime<Utc>) -> ApiResult<Vec<LinkReader>> {
    Ok(sqlx::query_as(
        "SELECT u.id, u.name, u.color, v.last_viewed_at FROM views v JOIN users u ON u.id = v.user_id \
         JOIN documents d ON d.id = v.document_id \
         WHERE v.document_id = $1 AND v.last_viewed_at >= $2 \
           AND NOT EXISTS (SELECT 1 FROM team_memberships tm WHERE tm.team_id = d.team_id AND tm.user_id = u.id) \
         ORDER BY v.last_viewed_at DESC LIMIT 100",
    )
    .bind(doc)
    .bind(since)
    .fetch_all(&st.db)
    .await?)
}

fn valid_audience(a: &str) -> ApiResult<()> {
    match a {
        "public" | "organization" => Ok(()),
        _ => Err(ApiError::bad_request("audience must be public or organization")),
    }
}

/// Links for the organization need the organization domains of the workspace
async fn require_organization_domains(st: &AppState, team_id: Uuid) -> ApiResult<()> {
    let (domains,): (String,) = sqlx::query_as("SELECT organization_domains FROM teams WHERE id = $1").bind(team_id).fetch_one(&st.db).await?;
    if domains.trim().is_empty() {
        return Err(ApiError::bad_request("Set the domains of your organization in Settings → Details first"));
    }
    Ok(())
}

pub async fn list(State(st): State<AppState>, auth: Auth) -> ApiResult<Json<Value>> {
    let rows: Vec<ShareRow> = sqlx::query_as(&format!(
        "{SHARE_SELECT} WHERE s.team_id = $1 AND s.revoked_at IS NULL AND d.deleted_at IS NULL \
         AND ($2 OR s.created_by = $3) ORDER BY s.created_at DESC"
    ))
    .bind(auth.team()?)
    .bind(auth.is_admin())
    .bind(auth.user.id)
    .fetch_all(&st.db)
    .await?;
    let out: Vec<Value> = rows.into_iter().map(|s| with_url(&st, s)).collect();
    ok(out)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocReq {
    document_id: String,
    /// shares.create: "public" (default) or "organization"
    #[serde(default)]
    audience: Option<String>,
}

/// Whether public links of documents in this workspace and collection work: sharing must be
/// allowed by the installation, the workspace and the collection (documents outside a collection
/// can't be shared). Same rules as documents.info with a share id.
pub async fn sharing_enabled(st: &AppState, team_id: Uuid, collection_id: Option<Uuid>) -> ApiResult<bool> {
    let Some(cid) = collection_id else { return Ok(false) };
    if !st.settings().allow_public_sharing {
        return Ok(false);
    }
    let row: Option<(bool, bool)> = sqlx::query_as(
        "SELECT t.sharing, c.sharing FROM teams t JOIN collections c ON c.team_id = t.id \
         WHERE t.id = $1 AND c.id = $2 AND c.deleted_at IS NULL",
    )
    .bind(team_id)
    .bind(cid)
    .fetch_optional(&st.db)
    .await?;
    Ok(row.is_some_and(|(team, collection)| team && collection))
}

/// When the working public link of the document was created, None without one
pub async fn public_since(st: &AppState, d: &crate::models::DocRow) -> ApiResult<Option<chrono::DateTime<chrono::Utc>>> {
    if d.published_at.is_none() || d.deleted_at.is_some() || !sharing_enabled(st, d.team_id, d.collection_id).await? {
        return Ok(None);
    }
    let (at,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT min(created_at) FROM shares WHERE document_id = $1 AND revoked_at IS NULL")
            .bind(d.id)
            .fetch_one(&st.db)
            .await?;
    Ok(at)
}

pub async fn info(State(st): State<AppState>, auth: Auth, Json(b): Json<DocReq>) -> ApiResult<Json<Value>> {
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    let row: Option<ShareRow> = sqlx::query_as(&format!(
        "{SHARE_SELECT} WHERE s.document_id = $1 AND s.revoked_at IS NULL ORDER BY s.created_at LIMIT 1"
    ))
    .bind(d.id)
    .fetch_optional(&st.db)
    .await?;
    match row {
        Some(s) => ok(share_json(&st, s).await?),
        None => ok(Value::Null),
    }
}

pub async fn create(State(st): State<AppState>, auth: Auth, Json(b): Json<DocReq>) -> ApiResult<Json<Value>> {
    auth.require_writer()?;
    let (d, _) = load_doc(&st.db, &auth, &b.document_id, Access::Read).await?;
    if d.published_at.is_none() {
        return Err(ApiError::bad_request("Publish the document before sharing it"));
    }
    if !st.settings().allow_public_sharing {
        return Err(ApiError::forbidden("Public links have been disabled by the administrator"));
    }
    let (sharing,): (bool,) = sqlx::query_as("SELECT sharing FROM teams WHERE id = $1")
        .bind(d.team_id)
        .fetch_one(&st.db)
        .await?;
    if !sharing {
        return Err(ApiError::forbidden("Public sharing is disabled for this workspace"));
    }
    if let Some(cid) = d.collection_id {
        let (cs,): (bool,) = sqlx::query_as("SELECT sharing FROM collections WHERE id = $1")
            .bind(cid)
            .fetch_one(&st.db)
            .await?;
        if !cs {
            return Err(ApiError::forbidden("Public sharing is disabled for this collection"));
        }
    }
    let existing: Option<ShareRow> = sqlx::query_as(&format!(
        "{SHARE_SELECT} WHERE s.document_id = $1 AND s.revoked_at IS NULL ORDER BY s.created_at LIMIT 1"
    ))
    .bind(d.id)
    .fetch_optional(&st.db)
    .await?;
    if let Some(s) = existing {
        return ok(share_json(&st, s).await?);
    }
    let audience = b.audience.as_deref().unwrap_or("public");
    valid_audience(audience)?;
    if audience == "organization" {
        require_organization_domains(&st, d.team_id).await?;
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO shares (id, url_id, team_id, document_id, created_by, audience) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(id)
        // long random token: public links must not be guessable
        .bind(random_string(24))
        .bind(d.team_id)
        .bind(d.id)
        .bind(auth.user.id)
        .bind(audience)
        .execute(&st.db)
        .await?;
    let s: ShareRow = sqlx::query_as(&format!("{SHARE_SELECT} WHERE s.id = $1"))
        .bind(id)
        .fetch_one(&st.db)
        .await?;
    audit::record_in(&st, &auth, Some(d.team_id), "shares.create", Target::document(d.id, &d.title), json!({ "audience": audience })).await;
    ok(share_json(&st, s).await?)
}

#[derive(Deserialize)]
pub struct IdReq {
    id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReq {
    id: Uuid,
    include_child_documents: Option<bool>,
    allow_indexing: Option<bool>,
    show_last_updated: Option<bool>,
    audience: Option<String>,
}

/// Changes the options of a public link; same people as revoking it.
pub async fn update(State(st): State<AppState>, auth: Auth, Json(b): Json<UpdateReq>) -> ApiResult<Json<Value>> {
    let d = manageable(&st, &auth, b.id).await?;
    if let Some(a) = &b.audience {
        valid_audience(a)?;
        if a == "organization" {
            require_organization_domains(&st, d.team_id).await?;
        }
    }
    // the pages of a link for the organization are never indexed
    sqlx::query(
        "UPDATE shares SET include_children = COALESCE($2, include_children), allow_indexing = COALESCE($3, allow_indexing), \
         show_last_updated = COALESCE($4, show_last_updated), audience = COALESCE($5, audience) WHERE id = $1",
    )
    .bind(b.id)
    .bind(b.include_child_documents)
    .bind(b.allow_indexing)
    .bind(b.show_last_updated)
    .bind(&b.audience)
    .execute(&st.db)
    .await?;
    sqlx::query("UPDATE shares SET allow_indexing = false WHERE id = $1 AND audience = 'organization'").bind(b.id).execute(&st.db).await?;
    let s: ShareRow = sqlx::query_as(&format!("{SHARE_SELECT} WHERE s.id = $1")).bind(b.id).fetch_one(&st.db).await?;
    let data = json!({
        "includeChildDocuments": s.include_child_documents,
        "allowIndexing": s.allow_indexing,
        "showLastUpdated": s.show_last_updated,
        "audience": s.audience,
    });
    audit::record_in(&st, &auth, Some(d.team_id), "shares.update", Target::document(d.id, &d.title), data).await;
    ok(share_json(&st, s).await?)
}

/// The document of a link that `auth` may change or revoke: who created it, admins, editors of the document
async fn manageable(st: &AppState, auth: &Auth, id: Uuid) -> ApiResult<crate::models::DocRow> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as("SELECT document_id, created_by FROM shares WHERE id = $1 AND revoked_at IS NULL")
        .bind(id)
        .fetch_optional(&st.db)
        .await?;
    let (doc_id, created_by) = row.ok_or_else(|| ApiError::not_found("Share not found"))?;
    let (d, access) = load_doc(&st.db, auth, &doc_id.to_string(), Access::Read).await?;
    if !(created_by == auth.user.id || auth.is_admin() || access == Access::Write) {
        return Err(ApiError::forbidden("You cannot change this share"));
    }
    Ok(d)
}

pub async fn revoke(State(st): State<AppState>, auth: Auth, Json(b): Json<IdReq>) -> ApiResult<Json<Value>> {
    let d = manageable(&st, &auth, b.id).await?;
    sqlx::query("UPDATE shares SET revoked_at = now() WHERE id = $1")
        .bind(b.id)
        .execute(&st.db)
        .await?;
    audit::record_in(&st, &auth, Some(d.team_id), "shares.revoke", Target::document(d.id, &d.title), json!({})).await;
    ok(json!({ "success": true }))
}

// ---------------------------------------------------------------------------
// public pages
// ---------------------------------------------------------------------------

/// A working public link, as seen by anyone who has it
pub struct PublicShare {
    pub id: Uuid,
    pub url_id: String,
    pub document: crate::models::DocRow,
    pub team_name: String,
    pub include_children: bool,
    pub allow_indexing: bool,
    pub show_last_updated: bool,
    /// "public" or "organization" (see `check_audience`)
    pub audience: String,
}

/// The public link `share` (its id or its token), None when it doesn't exist or doesn't work any
/// more: revoked, sharing turned off, the document unpublished or deleted.
pub async fn public_share(st: &AppState, share: &str) -> ApiResult<Option<PublicShare>> {
    if !st.settings().allow_public_sharing {
        return Ok(None);
    }
    type Row = (Uuid, String, Uuid, bool, bool, bool, String);
    const COLS: &str = "SELECT id, url_id, document_id, include_children, allow_indexing, show_last_updated, audience FROM shares";
    let row: Option<Row> = match crate::util::parse_id(share) {
        crate::util::IdRef::Uuid(u) => {
            sqlx::query_as(&format!("{COLS} WHERE id = $1 AND revoked_at IS NULL")).bind(u).fetch_optional(&st.db).await?
        }
        crate::util::IdRef::UrlId(u) => {
            sqlx::query_as(&format!("{COLS} WHERE url_id = $1 AND revoked_at IS NULL")).bind(u).fetch_optional(&st.db).await?
        }
    };
    let Some((id, url_id, doc_id, include_children, allow_indexing, show_last_updated, audience)) = row else { return Ok(None) };
    let d = crate::perm::find_doc(&st.db, Uuid::nil(), &doc_id.to_string()).await?;
    if d.published_at.is_none() || d.deleted_at.is_some() || !sharing_enabled(st, d.team_id, d.collection_id).await? {
        return Ok(None);
    }
    let (team_name,): (String,) = sqlx::query_as("SELECT name FROM teams WHERE id = $1").bind(d.team_id).fetch_one(&st.db).await?;
    // the pages of a link for the organization are never indexed
    let allow_indexing = allow_indexing && audience == "public";
    Ok(Some(PublicShare { id, url_id, document: d, team_name, include_children, allow_indexing, show_last_updated, audience }))
}

/// Whether `auth` may read the document of a link: anyone for public links. Links for the organization
/// need a member of the workspace, or a signed-in person whose email is verified (see `verified_email`)
/// and belongs to one of the organization domains of the workspace. Returns whether the reader is a
/// member of the workspace.
pub async fn check_audience(st: &AppState, auth: Option<&Auth>, share: &PublicShare) -> ApiResult<bool> {
    let team_id = share.document.team_id;
    let Some(auth) = auth else {
        if share.audience == "public" {
            return Ok(false);
        }
        return Err(ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Sign in to read this document",
        ));
    };
    let member: Option<(Uuid,)> =
        sqlx::query_as("SELECT user_id FROM team_memberships WHERE team_id = $1 AND user_id = $2 AND suspended_at IS NULL")
            .bind(team_id)
            .bind(auth.user.id)
            .fetch_optional(&st.db)
            .await?;
    if member.is_some() || share.audience == "public" {
        return Ok(member.is_some());
    }
    let (domains,): (String,) = sqlx::query_as("SELECT organization_domains FROM teams WHERE id = $1").bind(team_id).fetch_one(&st.db).await?;
    let domain = crate::api::sso::email_domain(&auth.user.email);
    let in_domain = crate::api::teams::domain_list(&domains).any(|d| d == domain);
    if in_domain && verified_email(st, &auth.user).await? {
        return Ok(false);
    }
    Err(ApiError::forbidden(format!(
        "Only the people of {} can read this document. Sign in with your work account.",
        share.team_name
    )))
}

/// Whether the email of the account was proven by a sign-in: with an identity provider that verifies
/// it (Google, Slack, OpenID Connect; Microsoft when the tenant is specific or the domains are
/// restricted) or with a sign-in link sent to it. Passwords and passkeys don't prove the address.
pub async fn verified_email(st: &AppState, user: &crate::models::User) -> ApiResult<bool> {
    let microsoft_trusted = st.cfg.microsoft.as_ref().is_some_and(crate::api::sso::email_trusted);
    let (verified,): (bool,) = sqlx::query_as(
        "SELECT ($2 AND EXISTS (SELECT 1 FROM users WHERE id = $1 AND microsoft_id IS NOT NULL)) \
            OR EXISTS (SELECT 1 FROM user_identities WHERE user_id = $1 AND lower(email) = lower($3)) \
            OR EXISTS (SELECT 1 FROM login_tokens WHERE user_id = $1 AND used_at IS NOT NULL)",
    )
    .bind(user.id)
    .bind(microsoft_trusted)
    .bind(&user.email)
    .fetch_one(&st.db)
    .await?;
    Ok(verified)
}

/// A document of a public link: the shared one, or one nested in it when the link includes them
#[derive(Serialize, FromRow, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SharedNode {
    pub id: Uuid,
    pub url_id: String,
    pub parent_document_id: Option<Uuid>,
    pub title: String,
    pub emoji: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// The published documents nested in `root` (not archived nor deleted), parents first
pub async fn shared_descendants(st: &AppState, root: Uuid) -> ApiResult<Vec<SharedNode>> {
    Ok(sqlx::query_as(
        "WITH RECURSIVE tree AS ( \
            SELECT d.id, d.url_id, d.parent_document_id, d.title, d.emoji, d.updated_at, d.position, 1 AS depth \
            FROM documents d WHERE d.parent_document_id = $1 \
              AND d.published_at IS NOT NULL AND d.deleted_at IS NULL AND d.archived_at IS NULL AND d.template = false \
            UNION ALL SELECT d.id, d.url_id, d.parent_document_id, d.title, d.emoji, d.updated_at, d.position, tree.depth + 1 \
            FROM documents d JOIN tree ON d.parent_document_id = tree.id \
            WHERE d.published_at IS NOT NULL AND d.deleted_at IS NULL AND d.archived_at IS NULL AND d.template = false AND tree.depth < 50) \
         SELECT id, url_id, parent_document_id, title, emoji, updated_at FROM tree ORDER BY depth, position, title",
    )
    .bind(root)
    .fetch_all(&st.db)
    .await?)
}

/// GET /s/:share/sitemap.xml - the pages of a public link that search engines may index
pub async fn sitemap(
    State(st): State<AppState>,
    axum::extract::Path(share): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::{http::{header, StatusCode}, response::IntoResponse};
    let share = share.trim_end_matches(".xml").to_string();
    let Ok(Some(s)) = public_share(&st, &share).await else { return StatusCode::NOT_FOUND.into_response() };
    if !s.allow_indexing {
        return StatusCode::NOT_FOUND.into_response();
    }
    let base = format!("{}/s/{}", st.cfg.url.trim_end_matches('/'), s.url_id);
    let mut pages = vec![(base.clone(), s.document.updated_at)];
    if s.include_children {
        if let Ok(children) = shared_descendants(&st, s.document.id).await {
            pages.extend(children.into_iter().map(|c| (format!("{base}/doc/{}", c.url_id), c.updated_at)));
        }
    }
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for (loc, at) in pages {
        xml.push_str(&format!("  <url><loc>{loc}</loc><lastmod>{}</lastmod></url>\n", at.format("%Y-%m-%d")));
    }
    xml.push_str("</urlset>\n");
    ([(header::CONTENT_TYPE, "application/xml; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=3600")], xml).into_response()
}

/// GET /s/:share and /s/:share/doc/:id - the app page. Unless the link allows it, the headers keep
/// it out of search engines; the address of the link never leaks through the Referer header.
pub async fn shared_page(State(st): State<AppState>, axum::extract::Path(params): axum::extract::Path<Vec<(String, String)>>) -> axum::response::Response {
    use axum::{http::{header, StatusCode}, response::IntoResponse};
    let share = params.iter().find(|(k, _)| k == "share").map(|(_, v)| v.as_str()).unwrap_or_default();
    let indexable = matches!(public_share(&st, share).await, Ok(Some(s)) if s.allow_indexing);
    let Ok(html) = tokio::fs::read(st.cfg.static_dir.join("index.html")).await else { return StatusCode::NOT_FOUND.into_response() };
    let mut res = (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        html,
    )
        .into_response();
    if !indexable {
        res.headers_mut().insert(header::HeaderName::from_static("x-robots-tag"), header::HeaderValue::from_static("noindex, nofollow"));
    }
    res
}
