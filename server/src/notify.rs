//! Email preferences per kind of notification (Settings → Notifications), as in Outline, and the
//! notifications that are not about followed documents or mentions: comments, resolved threads,
//! access to a collection and accepted invitations. Notifications always appear in the app; the
//! preferences only choose the emails.

use std::collections::HashSet;

use uuid::Uuid;

use crate::{
    auth::Auth,
    error::ApiResult,
    mail::esc,
    models::{Access, DocRow, User},
    perm::{collection_access, doc_access, find_collection},
    AppState,
};

/// The kinds of notification with an email preference, in the order of the settings page
pub const EVENTS: &[&str] = &[
    "mentioned_in_document",
    "document_published",
    "document_updated",
    "document_shared",
    "comment_created",
    "comment_resolved",
    "collection_added",
    "invite_accepted",
];

/// Whether `user` wants an email for a notification of kind `event` (on unless turned off)
pub fn wants_email(user: &User, event: &str) -> bool {
    let event = if event == "group_mentioned_in_document" { "mentioned_in_document" } else { event };
    user.notification_settings.get(event).and_then(|v| v.as_bool()).unwrap_or(true)
}

/// `user_id` as an active member of `team`, to check what they can read
async fn member(st: &AppState, user_id: Uuid, team: Uuid) -> ApiResult<Option<Auth>> {
    let Some(user) = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1 AND suspended_at IS NULL")
        .bind(user_id)
        .fetch_optional(&st.db)
        .await?
    else {
        return Ok(None);
    };
    let role: Option<(String,)> =
        sqlx::query_as("SELECT role FROM team_memberships WHERE user_id = $1 AND team_id = $2 AND suspended_at IS NULL")
            .bind(user_id)
            .bind(team)
            .fetch_optional(&st.db)
            .await?;
    Ok(role.map(|(role,)| Auth { user, team_id: Some(team), role: Some(role), session_hash: None, key: None, meta: Default::default() }))
}

/// What a notification is about
struct Notice<'a> {
    event: &'static str,
    team: Uuid,
    actor: &'a User,
    document: Option<&'a DocRow>,
    collection: Option<Uuid>,
    comment: Option<(Uuid, &'a str)>,
}

/// Records the notification for `user_id` and sends the email they want. Nobody is notified
/// about what they can't read.
async fn deliver(st: &AppState, user_id: Uuid, n: &Notice<'_>) -> ApiResult<()> {
    if user_id == n.actor.id {
        return Ok(());
    }
    let Some(as_user) = member(st, user_id, n.team).await? else { return Ok(()) };
    if let Some(d) = n.document {
        if doc_access(&st.db, &as_user, d).await? == Access::None {
            return Ok(());
        }
    }
    let collection = match n.collection {
        Some(c) => {
            let c = find_collection(&st.db, &c.to_string()).await?;
            if collection_access(&st.db, &as_user, &c).await? == Access::None {
                return Ok(());
            }
            Some(c)
        }
        None => None,
    };
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO notifications (id, user_id, team_id, actor_id, document_id, collection_id, comment_id, event) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(user_id)
    .bind(n.team)
    .bind(n.actor.id)
    .bind(n.document.map(|d| d.id))
    .bind(n.collection)
    .bind(n.comment.map(|c| c.0))
    .bind(n.event)
    .execute(&st.db)
    .await?;

    let user = &as_user.user;
    let Some(smtp) = st.cfg.smtp.as_ref().filter(|_| wants_email(user, n.event)) else { return Ok(()) };
    let (team,): (String,) = sqlx::query_as("SELECT name FROM teams WHERE id = $1").bind(n.team).fetch_one(&st.db).await?;
    let lang = user.language.clone().unwrap_or_else(|| st.defaults().language);
    let it = lang == "it";
    let settings = st.settings();
    let product = settings.product_name();
    let brand = settings.header_color.or(settings.accent_color).unwrap_or_else(|| "#1f2937".into());
    let base = st.cfg.url.trim_end_matches('/');
    let actor = n.actor.name.as_str();
    let doc_title = n
        .document
        .map(|d| d.title.trim())
        .map(|t| if t.is_empty() { if it { "Senza titolo" } else { "Untitled" } } else { t })
        .unwrap_or_default()
        .to_string();
    let quote = n.comment.filter(|(_, text)| !text.trim().is_empty()).map(|(_, text)| {
        let short: String = text.chars().take(300).collect();
        format!(
            "<span style=\"display:block;border-left:3px solid #e5e7eb;padding-left:12px;color:#4b5563;\">{}</span>",
            esc(&short).replace('\n', "<br>")
        )
    });
    let (subject, title, intro, button, link) = match (n.event, it) {
        ("comment_created", true) => (
            format!("{actor} ha commentato \"{doc_title}\""),
            format!("{actor} ha scritto un commento"),
            format!("<strong>{}</strong> ha commentato il documento <strong>{}</strong> del workspace {}.", esc(actor), esc(&doc_title), esc(&team)),
            "Apri il documento",
            n.document.map(|d| format!("{base}/doc/{}", d.url_id)),
        ),
        ("comment_created", false) => (
            format!("{actor} commented on \"{doc_title}\""),
            format!("{actor} left a comment"),
            format!("<strong>{}</strong> commented on the document <strong>{}</strong> of the {} workspace.", esc(actor), esc(&doc_title), esc(&team)),
            "Open the document",
            n.document.map(|d| format!("{base}/doc/{}", d.url_id)),
        ),
        ("comment_resolved", true) => (
            format!("{actor} ha risolto una discussione in \"{doc_title}\""),
            format!("{actor} ha risolto una discussione"),
            format!("<strong>{}</strong> ha segnato come risolta una discussione a cui hai partecipato nel documento <strong>{}</strong>.", esc(actor), esc(&doc_title)),
            "Apri il documento",
            n.document.map(|d| format!("{base}/doc/{}", d.url_id)),
        ),
        ("comment_resolved", false) => (
            format!("{actor} resolved a thread in \"{doc_title}\""),
            format!("{actor} resolved a thread"),
            format!("<strong>{}</strong> resolved a thread you took part in on the document <strong>{}</strong>.", esc(actor), esc(&doc_title)),
            "Open the document",
            n.document.map(|d| format!("{base}/doc/{}", d.url_id)),
        ),
        ("collection_added", it) => {
            let name = collection.as_ref().map(|c| c.name.clone()).unwrap_or_default();
            let link = collection.as_ref().map(|c| format!("{base}/collection/{}", c.url_id));
            if it {
                (
                    format!("{actor} ti ha dato accesso a \"{name}\""),
                    "Hai accesso a una nuova collezione".to_string(),
                    format!("<strong>{}</strong> ti ha aggiunto alla collezione <strong>{}</strong> del workspace {}.", esc(actor), esc(&name), esc(&team)),
                    "Apri la collezione",
                    link,
                )
            } else {
                (
                    format!("{actor} gave you access to \"{name}\""),
                    "You have access to a new collection".to_string(),
                    format!("<strong>{}</strong> added you to the collection <strong>{}</strong> of the {} workspace.", esc(actor), esc(&name), esc(&team)),
                    "Open the collection",
                    link,
                )
            }
        }
        ("invite_accepted", true) => (
            format!("{actor} ha accettato il tuo invito"),
            format!("{actor} è entrato nel workspace"),
            format!("<strong>{}</strong> ha accettato il tuo invito ed è entrato nel workspace {}.", esc(actor), esc(&team)),
            "Vedi i membri",
            Some(format!("{base}/settings/members?workspace={}", n.team)),
        ),
        ("invite_accepted", false) => (
            format!("{actor} accepted your invitation"),
            format!("{actor} joined the workspace"),
            format!("<strong>{}</strong> accepted your invitation and joined the {} workspace.", esc(actor), esc(&team)),
            "See the members",
            Some(format!("{base}/settings/members?workspace={}", n.team)),
        ),
        _ => return Ok(()),
    };
    let Some(link) = link else { return Ok(()) };
    let mut paragraphs = vec![intro];
    paragraphs.extend(quote);
    let mail = crate::mail::notice(&crate::mail::NoticeMail {
        lang: &lang,
        product: &product,
        subject,
        title,
        paragraphs,
        button,
        link: &link,
        base_url: base,
        brand_color: &brand,
    });
    smtp.send(&user.email, Some(&user.name), &mail).await?;
    sqlx::query("UPDATE notifications SET emailed_at = now() WHERE id = $1").bind(id).execute(&st.db).await?;
    Ok(())
}

fn log(kind: &str, r: ApiResult<()>) {
    if let Err(e) = r {
        tracing::warn!("{kind} notification failed: {}", e.message);
    }
}

/// A new comment: the followers of the document and who took part in the thread are notified,
/// in the background.
pub fn comment_created(st: &AppState, actor: &User, doc: &DocRow, comment: Uuid, thread: Uuid, text: &str) {
    if doc.published_at.is_none() || doc.template {
        return;
    }
    let (st, actor, doc, text) = (st.clone(), actor.clone(), doc.clone(), text.to_string());
    tokio::spawn(async move {
        let rows: Result<Vec<(Uuid,)>, _> = sqlx::query_as(
            "SELECT user_id FROM subscriptions WHERE active AND document_id = $1 \
             UNION SELECT created_by FROM comments WHERE id = $2 OR parent_comment_id = $2",
        )
        .bind(doc.id)
        .bind(thread)
        .fetch_all(&st.db)
        .await;
        let users: HashSet<Uuid> = match rows {
            Ok(r) => r.into_iter().map(|r| r.0).collect(),
            Err(e) => return tracing::warn!(error = %e, "cannot load who to notify about the comment"),
        };
        let n = Notice { event: "comment_created", team: doc.team_id, actor: &actor, document: Some(&doc), collection: None, comment: Some((comment, &text)) };
        for user in users {
            log("comment", deliver(&st, user, &n).await);
        }
    });
}

/// A thread was resolved: who took part in it is notified, in the background.
pub fn comment_resolved(st: &AppState, actor: &User, doc: &DocRow, thread: Uuid) {
    let (st, actor, doc) = (st.clone(), actor.clone(), doc.clone());
    tokio::spawn(async move {
        let rows: Result<Vec<(Uuid,)>, _> =
            sqlx::query_as("SELECT DISTINCT created_by FROM comments WHERE id = $1 OR parent_comment_id = $1").bind(thread).fetch_all(&st.db).await;
        let users = match rows {
            Ok(r) => r,
            Err(e) => return tracing::warn!(error = %e, "cannot load who to notify about the thread"),
        };
        let n = Notice { event: "comment_resolved", team: doc.team_id, actor: &actor, document: Some(&doc), collection: None, comment: Some((thread, "")) };
        for (user,) in users {
            log("resolved thread", deliver(&st, user, &n).await);
        }
    });
}

/// Someone was given access to a collection.
pub fn collection_added(st: &AppState, actor: &User, team: Uuid, collection: Uuid, user: Uuid) {
    let (st, actor) = (st.clone(), actor.clone());
    tokio::spawn(async move {
        let n = Notice { event: "collection_added", team, actor: &actor, document: None, collection: Some(collection), comment: None };
        log("collection access", deliver(&st, user, &n).await);
    });
}

/// `joined` accepted an invitation of `inviter` to `team`.
pub fn invite_accepted(st: &AppState, joined: &User, team: Uuid, inviter: Uuid) {
    let (st, joined) = (st.clone(), joined.clone());
    tokio::spawn(async move {
        let n = Notice { event: "invite_accepted", team, actor: &joined, document: None, collection: None, comment: None };
        log("accepted invitation", deliver(&st, inviter, &n).await);
    });
}
