//! Previews of external links (title, description, site), shown when the pointer rests on a link.
//! Off by default: when a superadmin turns it on, the server fetches the linked pages itself.
//!
//! Only http(s) on the standard ports; addresses that are not public (loopback, private,
//! link-local, …) are refused, also after every redirect, and the checked address is the one the
//! request connects to. At most 512 KB of HTML are read, with a 5 second timeout. Results, also
//! failed ones, are cached for a day.

use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::Auth,
    error::{ok, ApiError, ApiResult},
    AppState,
};

const MAX_BYTES: usize = 512 * 1024;
const MAX_REDIRECTS: usize = 3;

#[derive(Deserialize)]
pub struct UrlReq {
    url: String,
}

/// Addresses on the internet; everything else (the server itself, the local network…) is refused
pub(crate) fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (18..=19).contains(&o[1]))
                || o[0] >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80)
        }
    }
}

/// A public address of the host of `url`, or an error
async fn public_address(url: &reqwest::Url) -> Result<(String, SocketAddr), String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("unsupported scheme".into());
    }
    let host = url.host_str().ok_or("no host")?.to_string();
    let port = url.port_or_known_default().ok_or("no port")?;
    if port != 80 && port != 443 {
        return Err("unsupported port".into());
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port)).await.map_err(|e| e.to_string())?.collect();
    if addrs.is_empty() || addrs.iter().any(|a| !is_public(a.ip())) {
        return Err("address not allowed".into());
    }
    Ok((host, addrs[0]))
}

fn meta(doc: &scraper::Html, selectors: &[&str]) -> Option<String> {
    for s in selectors {
        let sel = scraper::Selector::parse(s).ok()?;
        if let Some(el) = doc.select(&sel).next() {
            let v = el.value().attr("content").map(str::to_string).unwrap_or_else(|| el.text().collect::<String>());
            let v = v.split_whitespace().collect::<Vec<_>>().join(" ");
            if !v.is_empty() {
                return Some(v.chars().take(300).collect());
            }
        }
    }
    None
}

async fn fetch(start: &str) -> Result<Value, String> {
    let mut url = reqwest::Url::parse(start.trim()).map_err(|e| e.to_string())?;
    for _ in 0..=MAX_REDIRECTS {
        let (host, addr) = public_address(&url).await?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .resolve(&host, addr)
            .user_agent("GetPaper link preview")
            .build()
            .map_err(|e| e.to_string())?;
        let mut resp = client.get(url.clone()).header("accept", "text/html").send().await.map_err(|e| e.to_string())?;
        if resp.status().is_redirection() {
            let loc = resp.headers().get("location").and_then(|v| v.to_str().ok()).ok_or("redirect without location")?;
            url = url.join(loc).map_err(|e| e.to_string())?;
            continue;
        }
        if !resp.status().is_success() {
            return Err(format!("status {}", resp.status()));
        }
        let html_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|c| c.contains("text/html"));
        if !html_type {
            return Err("not an HTML page".into());
        }
        let mut body: Vec<u8> = vec![];
        while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
            body.extend_from_slice(&chunk);
            if body.len() >= MAX_BYTES {
                break;
            }
        }
        let doc = scraper::Html::parse_document(&String::from_utf8_lossy(&body));
        let title = meta(&doc, &["meta[property='og:title']", "meta[name='twitter:title']", "title"]);
        let description = meta(&doc, &["meta[property='og:description']", "meta[name='description']", "meta[name='twitter:description']"]);
        let site = meta(&doc, &["meta[property='og:site_name']"]).or_else(|| url.host_str().map(|h| h.trim_start_matches("www.").to_string()));
        return Ok(json!({ "title": title, "description": description, "siteName": site, "url": url.as_str() }));
    }
    Err("too many redirects".into())
}

pub async fn unfurl(State(st): State<AppState>, auth: Auth, Json(b): Json<UrlReq>) -> ApiResult<Json<Value>> {
    let _ = auth.team()?;
    if !st.settings().link_previews {
        return Err(ApiError::forbidden("Link previews are turned off"));
    }
    let url = b.url.trim().to_string();
    if url.len() > 2000 {
        return Err(ApiError::bad_request("The link is too long"));
    }
    let cached: Option<(Value,)> =
        sqlx::query_as("SELECT data FROM link_previews WHERE url = $1 AND fetched_at > now() - interval '1 day'")
            .bind(&url)
            .fetch_optional(&st.db)
            .await?;
    if let Some((data,)) = cached {
        return ok(data);
    }
    let data = fetch(&url).await.unwrap_or_else(|e| {
        tracing::debug!(url, error = e, "link preview not available");
        json!({})
    });
    sqlx::query(
        "INSERT INTO link_previews (url, data, fetched_at) VALUES ($1, $2, now()) \
         ON CONFLICT (url) DO UPDATE SET data = EXCLUDED.data, fetched_at = now()",
    )
    .bind(&url)
    .bind(&data)
    .execute(&st.db)
    .await?;
    ok(data)
}

#[cfg(test)]
mod tests {
    use super::is_public;

    #[test]
    fn only_public_addresses() {
        for ip in ["127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "0.0.0.0", "::1", "fd00::1", "fe80::1", "::ffff:127.0.0.1"] {
            assert!(!is_public(ip.parse().unwrap()), "{ip} must be refused");
        }
        for ip in ["93.184.216.34", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(is_public(ip.parse().unwrap()), "{ip} must be allowed");
        }
    }
}
