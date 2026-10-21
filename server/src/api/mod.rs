pub mod activity;
pub mod api_keys;
pub mod attachments;
pub mod backup_restore;
pub mod client_log;
pub mod collections;
pub mod document_shares;
pub mod comments;
pub mod documents;
pub mod export;
pub mod groups;
pub mod import;
pub mod import_confluence;
pub mod import_confluence_xml;
pub mod import_wikijs;
pub mod mcp;
pub mod instance;
pub mod notifications;
pub mod invites;
pub mod revisions;
pub mod session;
pub mod sso;
pub mod shares;
pub mod stars;
pub mod subscriptions;
pub mod teams;
pub mod unfurl;
pub mod users;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Request},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};

use crate::AppState;

/// Reads the whole request body before the handler runs. Handlers that ignore the body (e.g.
/// invites.list) would otherwise answer an `Expect: 100-continue` request without "100 Continue"
/// and close the connection; Microsoft Entra application proxy then cut longer answers short.
/// Uploads (multipart) are read by their handlers and are left alone.
async fn read_body_first(req: Request, next: Next) -> Response {
    let multipart = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/"));
    if multipart {
        return next.run(req).await;
    }
    let (parts, body) = req.into_parts();
    match axum::body::to_bytes(body, 32 * 1024 * 1024).await {
        Ok(bytes) => next.run(Request::from_parts(parts, Body::from(bytes))).await,
        Err(_) => (StatusCode::PAYLOAD_TOO_LARGE, "Request body too large").into_response(),
    }
}

pub fn router() -> Router<AppState> {
    let import_limit: usize = std::env::var("IMPORT_MAX_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(500);
    Router::new()
        .route("/workspace.export", post(export::export))
        .route("/client.error", post(client_log::log))
        .route("/instance.info", post(instance::info))
        .route("/instance.update", post(instance::update))
        .route("/instance.events", post(activity::instance_events))
        // activity and audit log of the workspace (admins)
        .route("/activity.changes", post(activity::changes))
        .route("/activity.people", post(activity::people))
        .route("/activity.events", post(activity::events))
        .route("/instance.addSuperadmin", post(instance::add_superadmin))
        .route("/instance.removeSuperadmin", post(instance::remove_superadmin))
        .route("/instance.testEmail", post(instance::test_email))
        .route(
            "/collections.import",
            post(import::import).layer(DefaultBodyLimit::max(import_limit * 1024 * 1024)),
        )
        // auth
        .route("/auth.config", post(session::config))
        .route("/auth.login", post(session::login))
        .route("/auth.register", post(session::register))
        .route("/auth.logout", post(session::logout))
        .route("/auth.info", post(session::info))
        // workspaces
        .route("/teams.info", post(teams::info))
        .route("/teams.list", post(teams::list))
        .route("/teams.create", post(teams::create))
        .route("/teams.switch", post(teams::switch))
        .route("/teams.update", post(teams::update))
        .route("/teams.delete", post(teams::delete))
        // users
        .route("/users.list", post(users::list))
        .route("/users.update", post(users::update))
        .route("/users.updateRole", post(users::update_role))
        .route("/users.remove", post(users::remove))
        .route("/users.suspend", post(users::suspend))
        .route("/users.activate", post(users::activate))
        .route("/users.suspendAccount", post(users::suspend_account))
        .route("/users.activateAccount", post(users::activate_account))
        .route("/users.changePassword", post(users::change_password))
        // invites
        .route("/invites.create", post(invites::create))
        .route("/invites.list", post(invites::list))
        .route("/invites.revoke", post(invites::revoke))
        .route("/invites.send", post(invites::send))
        .route("/invites.link", post(invites::link))
        .route("/invites.info", post(invites::info))
        .route("/invites.accept", post(invites::accept))
        // groups
        .route("/groups.list", post(groups::list))
        .route("/groups.info", post(groups::info))
        .route("/groups.create", post(groups::create))
        .route("/groups.update", post(groups::update))
        .route("/groups.delete", post(groups::delete))
        .route("/groups.memberships", post(groups::memberships))
        .route("/groups.addUser", post(groups::add_user))
        .route("/groups.removeUser", post(groups::remove_user))
        // collections
        .route("/collections.list", post(collections::list))
        .route("/collections.listForWorkspace", post(collections::list_for_workspace))
        .route("/collections.info", post(collections::info))
        .route("/collections.create", post(collections::create))
        .route("/collections.update", post(collections::update))
        .route("/collections.delete", post(collections::delete))
        .route("/collections.move", post(collections::move_collection))
        .route("/collections.documents", post(collections::documents))
        .route("/collections.memberships", post(collections::memberships))
        .route("/collections.addUser", post(collections::add_user))
        .route("/collections.removeUser", post(collections::remove_user))
        .route("/collections.groupMemberships", post(collections::group_memberships))
        .route("/collections.addGroup", post(collections::add_group))
        .route("/collections.removeGroup", post(collections::remove_group))
        .route("/collections.export", post(collections::export))
        // documents
        .route("/documents.list", post(documents::list))
        .route("/documents.drafts", post(documents::drafts))
        .route("/documents.viewed", post(documents::viewed))
        .route("/documents.archived", post(documents::archived))
        .route("/documents.deleted", post(documents::deleted))
        .route("/documents.templates", post(documents::templates))
        .route("/documents.info", post(documents::info))
        .route("/documents.create", post(documents::create))
        .route("/documents.update", post(documents::update))
        .route("/documents.move", post(documents::move_doc))
        .route("/documents.duplicate", post(documents::duplicate))
        .route("/documents.archive", post(documents::archive))
        .route("/documents.unpublish", post(documents::unpublish))
        .route("/documents.restore", post(documents::restore))
        .route("/documents.delete", post(documents::delete))
        .route("/documents.search", post(documents::search))
        .route("/documents.export", post(documents::export))
        .route("/documents.import", post(documents::import))
        // revisions
        .route("/revisions.list", post(revisions::list))
        .route("/revisions.info", post(revisions::info))
        // sharing a document with people and groups
        .route("/documents.memberships", post(document_shares::memberships))
        .route("/documents.addUser", post(document_shares::add_user))
        .route("/documents.removeUser", post(document_shares::remove_user))
        .route("/documents.addGroup", post(document_shares::add_group))
        .route("/documents.removeGroup", post(document_shares::remove_group))
        .route("/documents.sharedWithMe", post(document_shares::shared_with_me))
        // following documents and collections
        .route("/subscriptions.info", post(subscriptions::info))
        .route("/subscriptions.create", post(subscriptions::create))
        .route("/subscriptions.delete", post(subscriptions::delete))
        .route("/urls.unfurl", post(unfurl::unfurl))
        // stars
        .route("/stars.list", post(stars::list))
        .route("/stars.create", post(stars::create))
        .route("/stars.update", post(stars::update))
        .route("/stars.delete", post(stars::delete))
        // shares
        .route("/shares.list", post(shares::list))
        .route("/shares.info", post(shares::info))
        .route("/shares.create", post(shares::create))
        .route("/shares.revoke", post(shares::revoke))
        // comments
        .route("/comments.list", post(comments::list))
        .route("/comments.create", post(comments::create))
        .route("/comments.update", post(comments::update))
        .route("/comments.delete", post(comments::delete))
        .route("/comments.resolve", post(comments::resolve))
        // notifications
        .route("/notifications.list", post(notifications::list))
        .route("/notifications.markRead", post(notifications::mark_read))
        // attachments
        .route("/attachments.create", post(attachments::create))
        // api keys
        .route("/apiKeys.list", post(api_keys::list))
        .route("/apiKeys.create", post(api_keys::create))
        .route("/apiKeys.delete", post(api_keys::delete))
        .route("/apiKeys.collections", post(api_keys::collections))
        .layer(middleware::from_fn(read_body_first))
}
