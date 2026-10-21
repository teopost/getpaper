//! Following documents and collections, as in Outline. Notifications (and emails, if the person
//! wants them) when a followed document gets a new revision, when a document is published in a
//! followed collection and when a document is shared with someone.
//!
//! A new revision is a new editing session: saves of the same person within 5 minutes don't
//! notify again. While a "document updated" notification is unread, later updates refresh it
//! instead of adding new ones, and the email is sent only for the first.

use std::collections::HashSet;

use uuid::Uuid;

use crate::{
    auth::Auth,
    error::ApiResult,
    models::{Access, DocRow, User},
    perm::doc_access,
    AppState,
};

/// What happened to the document, for the notification and the email
#[derive(Clone, Copy, PartialEq)]
pub enum Event {
    Updated,
    Published,
    Shared,
}

impl Event {
    fn name(self) -> &'static str {
        match self {
            Event::Updated => "document_updated",
            Event::Published => "document_published",
            Event::Shared => "document_shared",
        }
    }
}

/// Follows a document on behalf of someone who published or edited it, unless they stopped
/// following it before.
pub async fn follow_automatically(st: &AppState, user: Uuid, doc: &DocRow) {
    if doc.template {
        return;
    }
    let r = sqlx::query("INSERT INTO subscriptions (user_id, document_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(user)
        .bind(doc.id)
        .execute(&st.db)
        .await;
    if let Err(e) = r {
        tracing::warn!(error = %e, "cannot follow the document");
    }
}

/// Notifies, in the background, the followers of a document (and of its collection) that it
/// changed or was published. Never the author of the change.
pub fn notify(st: &AppState, actor: &User, doc: &DocRow, event: Event) {
    if doc.published_at.is_none() || doc.deleted_at.is_some() || doc.template {
        return;
    }
    let (st, actor, doc) = (st.clone(), actor.clone(), doc.clone());
    tokio::spawn(async move {
        // a new document interests the followers of the collection; changes also those of the document
        let sql = match event {
            Event::Published => "SELECT DISTINCT user_id FROM subscriptions WHERE active AND collection_id = $2 AND user_id <> $3",
            _ => "SELECT DISTINCT user_id FROM subscriptions WHERE active AND (document_id = $1 OR collection_id = $2) AND user_id <> $3",
        };
        let followers: Vec<(Uuid,)> = match sqlx::query_as(sql).bind(doc.id).bind(doc.collection_id).bind(actor.id).fetch_all(&st.db).await {
            Ok(f) => f,
            Err(e) => return tracing::warn!(error = %e, "cannot load the followers"),
        };
        let mut seen = HashSet::new();
        for (user_id,) in followers {
            if seen.insert(user_id) {
                if let Err(e) = notify_user(&st, &actor, &doc, user_id, event).await {
                    tracing::warn!("subscription notification for {user_id} failed: {}", e.message);
                }
            }
        }
    });
}

/// Tells someone a document was shared with them.
pub async fn notify_shared(st: &AppState, actor: &User, doc: &DocRow, user_id: Uuid) {
    if user_id == actor.id {
        return;
    }
    if let Err(e) = notify_user(st, actor, doc, user_id, Event::Shared).await {
        tracing::warn!("share notification for {user_id} failed: {}", e.message);
    }
}

async fn notify_user(st: &AppState, actor: &User, doc: &DocRow, user_id: Uuid, event: Event) -> ApiResult<()> {
    let Some(user) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1 AND suspended_at IS NULL")
        .bind(user_id)
        .fetch_optional(&st.db)
        .await?
    else {
        return Ok(());
    };
    let role: Option<(String,)> =
        sqlx::query_as("SELECT role FROM team_memberships WHERE user_id = $1 AND team_id = $2 AND suspended_at IS NULL")
            .bind(user_id)
            .bind(doc.team_id)
            .fetch_optional(&st.db)
            .await?;
    let Some((role,)) = role else { return Ok(()) };
    let as_user = Auth { user: user.clone(), team_id: Some(doc.team_id), role: Some(role), session_hash: None, key: None, meta: Default::default() };
    if doc_access(&st.db, &as_user, doc).await? == Access::None {
        return Ok(());
    }

    // an unread "updated" notification is refreshed, not repeated
    if event == Event::Updated {
        let refreshed = sqlx::query(
            "UPDATE notifications SET actor_id = $3, created_at = now() WHERE user_id = $1 AND document_id = $2 \
             AND event = 'document_updated' AND viewed_at IS NULL",
        )
        .bind(user_id)
        .bind(doc.id)
        .bind(actor.id)
        .execute(&st.db)
        .await?;
        if refreshed.rows_affected() > 0 {
            return Ok(());
        }
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO notifications (id, user_id, team_id, actor_id, document_id, event) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(id)
        .bind(user_id)
        .bind(doc.team_id)
        .bind(actor.id)
        .bind(doc.id)
        .bind(event.name())
        .execute(&st.db)
        .await?;

    let Some(smtp) = st.cfg.smtp.as_ref().filter(|_| user.email_on_subscription) else { return Ok(()) };
    let (team,): (String,) = sqlx::query_as("SELECT name FROM teams WHERE id = $1").bind(doc.team_id).fetch_one(&st.db).await?;
    let lang = user.language.clone().unwrap_or_else(|| st.defaults().language);
    let settings = st.settings();
    let product = settings.product_name();
    let brand = settings.header_color.or(settings.accent_color).unwrap_or_else(|| "#1f2937".into());
    let base = st.cfg.url.trim_end_matches('/');
    let link = format!("{base}/doc/{}", doc.url_id);
    let mail = crate::mail::document_event(&crate::mail::DocumentEventMail {
        lang: &lang,
        product: &product,
        actor: &actor.name,
        document: &doc.title,
        team: &team,
        event: match event {
            Event::Updated => "updated",
            Event::Published => "published",
            Event::Shared => "shared",
        },
        link: &link,
        base_url: base,
        brand_color: &brand,
    });
    smtp.send(&user.email, Some(&user.name), &mail).await?;
    sqlx::query("UPDATE notifications SET emailed_at = now() WHERE id = $1").bind(id).execute(&st.db).await?;
    Ok(())
}
