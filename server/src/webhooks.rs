//! Webhooks of a workspace, as in Outline: for every event of the workspace (the ones of the audit
//! log, plus documents.update and comments.*) an HTTP POST with a JSON body goes to the webhooks
//! that chose it. The body is signed with the secret of the webhook:
//!
//! `X-GetPaper-Signature: t=<unix time>,s=<hex HMAC-SHA256 of "<unix time>.<body>">`
//!
//! Delivery happens in the background, with up to 3 attempts (after 10 and 60 seconds) and a 10
//! second timeout; redirects are not followed. Only public addresses are called unless
//! WEBHOOKS_ALLOW_PRIVATE=true. After 25 failed deliveries in a row a webhook is turned off.

use std::{net::SocketAddr, time::Duration};

use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use uuid::Uuid;

use crate::{
    audit::{Actor, Target},
    auth::Auth,
    AppState,
};

/// Events a webhook can choose, by group: "documents" covers every "documents.*" event
pub const GROUPS: &[&str] = &["documents", "collections", "comments", "shares", "users", "groups", "invites", "api_keys", "teams", "data"];

const MAX_FAILURES: i32 = 25;
const RETRIES: [u64; 2] = [10, 60];

/// Whether a webhook that chose `events` (empty: all) receives `event`
pub fn wants(events: &[String], event: &str) -> bool {
    events.is_empty() || events.iter().any(|e| e == event || event.strip_prefix(e.as_str()).is_some_and(|r| r.starts_with('.')))
}

/// `t=<time>,s=<signature>` for a body
pub fn signature(secret: &str, time: i64, body: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("any key length");
    mac.update(format!("{time}.{body}").as_bytes());
    format!("t={time},s={}", hex::encode(mac.finalize().into_bytes()))
}

fn allow_private() -> bool {
    std::env::var("WEBHOOKS_ALLOW_PRIVATE").is_ok_and(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

/// The address to call for `url`, checked: http(s), and public unless private ones are allowed
pub async fn resolve(url: &str) -> Result<(reqwest::Url, String, Vec<SocketAddr>), String> {
    let url = reqwest::Url::parse(url.trim()).map_err(|_| "invalid URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http and https URLs are allowed".into());
    }
    let host = url.host_str().ok_or("the URL has no host")?.trim_matches(['[', ']']).to_string();
    let port = url.port_or_known_default().ok_or("the URL has no port")?;
    let addrs: Vec<SocketAddr> =
        tokio::net::lookup_host((host.as_str(), port)).await.map_err(|e| format!("cannot resolve {host}: {e}"))?.collect();
    if addrs.is_empty() {
        return Err(format!("cannot resolve {host}"));
    }
    if !allow_private() && addrs.iter().any(|a| !crate::api::unfurl::is_public(a.ip())) {
        return Err("the address is not public (set WEBHOOKS_ALLOW_PRIVATE=true to allow it)".into());
    }
    Ok((url, host, addrs))
}

/// One POST; the HTTP status, or why it failed
async fn post(url: &str, headers: &[(&str, String)], body: &str) -> Result<u16, String> {
    let (url, host, addrs) = resolve(url).await?;
    // the request connects to the checked addresses
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&host, &addrs)
        .user_agent("GetPaper-Webhooks/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.post(url).header("content-type", "application/json").body(body.to_string());
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let res = req.send().await.map_err(|e| if e.is_timeout() { "timed out".to_string() } else { e.without_url().to_string() })?;
    Ok(res.status().as_u16())
}

/// Delivers `payload` to one webhook, with retries, and records the outcome: the HTTP status and the error.
pub async fn deliver(st: &AppState, webhook: Uuid, url: &str, secret: &str, event: &str, mut payload: Value) -> (Option<u16>, Option<String>) {
    let id = Uuid::new_v4();
    payload["id"] = json!(id);
    let body = payload.to_string();
    let started = std::time::Instant::now();
    let mut attempts = 0;
    let mut outcome: (Option<u16>, Option<String>);
    loop {
        attempts += 1;
        let time = chrono::Utc::now().timestamp();
        let headers = [
            ("x-getpaper-event", event.to_string()),
            ("x-getpaper-delivery", id.to_string()),
            ("x-getpaper-signature", signature(secret, time, &body)),
        ];
        outcome = match post(url, &headers, &body).await {
            Ok(s) if (200..300).contains(&s) => (Some(s), None),
            Ok(s) => (Some(s), Some(format!("HTTP {s}"))),
            Err(e) => (None, Some(e)),
        };
        // a test is tried once; errors of the receiver (4xx) are not retried
        let retry = event != "webhooks.test" && outcome.1.is_some() && !outcome.0.is_some_and(|s| (400..500).contains(&s) && s != 429);
        match RETRIES.get(attempts - 1) {
            Some(wait) if retry => tokio::time::sleep(Duration::from_secs(*wait)).await,
            _ => break,
        }
    }
    let ok = outcome.1.is_none();
    let _ = sqlx::query(
        "INSERT INTO webhook_deliveries (id, webhook_id, event, status, error, attempts, duration_ms) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(webhook)
    .bind(event)
    .bind(outcome.0.map(i32::from))
    .bind(&outcome.1)
    .bind(attempts as i32)
    .bind(started.elapsed().as_millis().min(i32::MAX as u128) as i32)
    .execute(&st.db)
    .await;
    if event != "webhooks.test" {
        let r: Result<Option<(bool,)>, _> = sqlx::query_as(
            "UPDATE webhooks SET failures = CASE WHEN $2 THEN 0 ELSE failures + 1 END, \
             enabled = enabled AND (CASE WHEN $2 THEN 0 ELSE failures + 1 END) < $3 WHERE id = $1 RETURNING enabled",
        )
        .bind(webhook)
        .bind(ok)
        .bind(MAX_FAILURES)
        .fetch_optional(&st.db)
        .await;
        if matches!(r, Ok(Some((false,)))) && !ok {
            tracing::warn!(%webhook, "webhook turned off after {MAX_FAILURES} failed deliveries");
        }
    }
    outcome
}

/// The body of a webhook call
pub fn payload(event: &str, team: (Uuid, &str), actor: &Actor<'_>, target: Option<&Target>, data: &Value, base: &str) -> Value {
    let target = target.map(|t| {
        let mut v = json!({ "type": t.kind, "id": t.id, "name": t.name });
        if let (Some(id), "document") = (t.id, t.kind) {
            v["url"] = json!(format!("{base}/doc/{id}"));
        }
        v
    });
    json!({
        "event": event,
        "createdAt": chrono::Utc::now(),
        "workspace": { "id": team.0, "name": team.1 },
        "actor": actor.id.map(|id| json!({ "id": id, "name": actor.name, "email": actor.email })),
        "target": target,
        "data": data,
    })
}

/// Sends an event of workspace `team` to its webhooks, in the background.
pub fn send(st: &AppState, team: Uuid, event: &str, actor: Actor<'_>, target: Option<&Target>, data: &Value) {
    let base = st.cfg.url.trim_end_matches('/').to_string();
    let (st, event) = (st.clone(), event.to_string());
    let actor = (actor.id, actor.name.map(str::to_string), actor.email.map(str::to_string));
    let target = target.map(|t| (t.kind, t.id, t.name.clone()));
    let data = data.clone();
    tokio::spawn(async move {
        let hooks: Vec<(Uuid, String, String, Vec<String>)> =
            match sqlx::query_as("SELECT id, url, secret, events FROM webhooks WHERE team_id = $1 AND enabled").bind(team).fetch_all(&st.db).await {
                Ok(h) => h,
                Err(e) => return tracing::warn!(error = %e, "cannot load the webhooks"),
            };
        let hooks: Vec<_> = hooks.into_iter().filter(|h| wants(&h.3, &event)).collect();
        if hooks.is_empty() {
            return;
        }
        let name: String = sqlx::query_as::<_, (String,)>("SELECT name FROM teams WHERE id = $1")
            .bind(team)
            .fetch_one(&st.db)
            .await
            .map(|r| r.0)
            .unwrap_or_default();
        let actor = Actor { id: actor.0, name: actor.1.as_deref(), email: actor.2.as_deref() };
        let target = target.map(|(kind, id, name)| Target { kind, id, name });
        let body = payload(&event, (team, &name), &actor, target.as_ref(), &data, &base);
        for (id, url, secret, _) in hooks {
            let (st, event, body) = (st.clone(), event.clone(), body.clone());
            tokio::spawn(async move {
                deliver(&st, id, &url, &secret, &event, body).await;
            });
        }
    });
}

/// An event that is not in the audit log (documents.update, comments.*), by the signed-in user
pub fn dispatch(st: &AppState, auth: &Auth, team: Uuid, event: &str, target: Option<Target>, data: Value) {
    let actor = Actor { id: Some(auth.user.id), name: Some(&auth.user.name), email: Some(&auth.user.email) };
    send(st, team, event, actor, target.as_ref(), &data);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chosen_events() {
        assert!(wants(&[], "documents.publish"));
        assert!(wants(&["documents".into()], "documents.publish"));
        assert!(wants(&["documents.publish".into()], "documents.publish"));
        assert!(!wants(&["documents".into()], "documents_x.publish"));
        assert!(!wants(&["collections".into()], "documents.publish"));
    }

    #[test]
    fn signature_is_hmac_of_time_and_body() {
        // python3 -c 'import hmac,hashlib; print(hmac.new(b"secret", b"1700000000.{\"a\":1}", hashlib.sha256).hexdigest())'
        assert_eq!(signature("secret", 1_700_000_000, r#"{"a":1}"#), "t=1700000000,s=49f24e537407743fa4a0242bb63b94b9a47ee99cbbe071ccd8a22550ae411686");
    }
}
