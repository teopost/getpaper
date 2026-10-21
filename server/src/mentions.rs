//! Mentions in documents, stored in the markdown as `@[Label](mention://<id>/<type>/<model id>)`
//! (the format Outline uses), where type is user, group, document or collection. Every mention has
//! its own id, so a mention added to a document is told apart from one that was already there,
//! even when the same person is mentioned again. People and groups (all of their members) are
//! notified; documents and collections are only references.

use std::{collections::HashSet, sync::LazyLock};

use regex::Regex;
use uuid::Uuid;

use crate::{
    auth::Auth,
    error::ApiResult,
    models::{Access, DocRow, User},
    perm::doc_access,
    AppState,
};

static MENTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@\[(?:[^\]\\]|\\.)*\]\(mention://([A-Za-z0-9_-]+)/([a-z_]+)/([0-9a-fA-F-]{36})\)").unwrap()
});

pub struct Mention {
    pub id: String,
    pub kind: String,
    pub model_id: Uuid,
}

pub fn parse(text: &str) -> Vec<Mention> {
    MENTION
        .captures_iter(text)
        .filter_map(|c| {
            Some(Mention { id: c[1].to_string(), kind: c[2].to_string(), model_id: Uuid::parse_str(&c[3]).ok()? })
        })
        .collect()
}

/// Notifies, in the background, the people mentioned in `doc` since `before` (the previous text;
/// None when the document has just been published, so every mention counts), directly or through
/// a group. Like Outline, only published documents notify, never the author of the change, only
/// people who can read the document, and nobody through a group with mentions turned off.
pub fn notify(st: &AppState, actor: &User, doc: &DocRow, before: Option<&str>) {
    if doc.published_at.is_none() || doc.deleted_at.is_some() || doc.template {
        return;
    }
    let old: HashSet<String> = before.map(|t| parse(t).into_iter().map(|m| m.id).collect()).unwrap_or_default();
    let added: Vec<Mention> = parse(&doc.text).into_iter().filter(|m| !old.contains(&m.id)).collect();
    let mut seen = HashSet::new();
    let pick = |kind: &str, seen: &mut HashSet<Uuid>| -> Vec<Uuid> {
        added.iter().filter(|m| m.kind == kind).map(|m| m.model_id).filter(|id| seen.insert(*id)).collect()
    };
    let users = pick("user", &mut seen);
    let groups = pick("group", &mut seen);
    if users.is_empty() && groups.is_empty() {
        return;
    }
    let (st, actor, doc) = (st.clone(), actor.clone(), doc.clone());
    tokio::spawn(async move {
        // people mentioned both directly and through a group are notified once
        let mut notified: HashSet<Uuid> = HashSet::from([actor.id]);
        for user_id in users {
            if notified.insert(user_id) {
                if let Err(e) = notify_user(&st, &actor, &doc, user_id, None).await {
                    tracing::warn!("mention notification for {user_id} failed: {}", e.message);
                }
            }
        }
        for group_id in groups {
            let group: Option<(String, bool)> =
                match sqlx::query_as("SELECT name, disable_mentions FROM groups WHERE id = $1 AND team_id = $2")
                    .bind(group_id)
                    .bind(doc.team_id)
                    .fetch_optional(&st.db)
                    .await
                {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::warn!("group mention {group_id}: {e}");
                        continue;
                    }
                };
            let Some((name, false)) = group else { continue };
            let members: Vec<(Uuid,)> = match sqlx::query_as("SELECT user_id FROM group_users WHERE group_id = $1")
                .bind(group_id)
                .fetch_all(&st.db)
                .await
            {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("group mention {group_id}: {e}");
                    continue;
                }
            };
            for (user_id,) in members {
                if notified.insert(user_id) {
                    if let Err(e) = notify_user(&st, &actor, &doc, user_id, Some((group_id, &name))).await {
                        tracing::warn!("group mention notification for {user_id} failed: {}", e.message);
                    }
                }
            }
        }
    });
}

/// `group`: the mentioned group the user belongs to, None for a direct mention
async fn notify_user(st: &AppState, actor: &User, doc: &DocRow, user_id: Uuid, group: Option<(Uuid, &str)>) -> ApiResult<()> {
    let Some(user) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1").bind(user_id).fetch_optional(&st.db).await? else {
        return Ok(());
    };
    let role: Option<(String,)> = sqlx::query_as("SELECT role FROM team_memberships WHERE user_id = $1 AND team_id = $2 AND suspended_at IS NULL")
        .bind(user_id)
        .bind(doc.team_id)
        .fetch_optional(&st.db)
        .await?;
    let Some((role,)) = role else { return Ok(()) };
    let as_user = Auth { user: user.clone(), team_id: Some(doc.team_id), role: Some(role), session_hash: None, key: None, meta: Default::default() };
    if doc_access(&st.db, &as_user, doc).await? == Access::None {
        return Ok(());
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO notifications (id, user_id, team_id, actor_id, document_id, event, group_id) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(user_id)
    .bind(doc.team_id)
    .bind(actor.id)
    .bind(doc.id)
    .bind(if group.is_some() { "group_mentioned_in_document" } else { "mentioned_in_document" })
    .bind(group.map(|g| g.0))
    .execute(&st.db)
    .await?;

    let Some(smtp) = st.cfg.smtp.as_ref().filter(|_| user.email_on_mention) else { return Ok(()) };
    let (team,): (String,) = sqlx::query_as("SELECT name FROM teams WHERE id = $1").bind(doc.team_id).fetch_one(&st.db).await?;
    let lang = user.language.clone().unwrap_or_else(|| st.defaults().language);
    let settings = st.settings();
    let product = settings.product_name();
    let brand = settings.header_color.or(settings.accent_color).unwrap_or_else(|| "#1f2937".into());
    let base = st.cfg.url.trim_end_matches('/');
    let link = format!("{base}/doc/{}", doc.url_id);
    let mail = crate::mail::mention(&crate::mail::MentionMail {
        lang: &lang,
        product: &product,
        actor: &actor.name,
        document: &doc.title,
        group: group.map(|g| g.1),
        team: &team,
        link: &link,
        base_url: base,
        brand_color: &brand,
    });
    smtp.send(&user.email, Some(&user.name), &mail).await?;
    sqlx::query("UPDATE notifications SET emailed_at = now() WHERE id = $1").bind(id).execute(&st.db).await?;
    Ok(())
}
