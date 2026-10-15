//! Outgoing email over SMTP (SMTP_HOST, SMTP_PORT, SMTP_USERNAME, SMTP_PASSWORD, SMTP_FROM_EMAIL,
//! SMTP_FROM_NAME, SMTP_SECURE) and the messages the application sends: workspace invitations.

use std::time::Duration;

use lettre::{
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

use crate::error::{ApiError, ApiResult};

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_email: String,
    /// SMTP_FROM_NAME; when not set the product name of the email is used
    pub from_name: Option<String>,
    /// "ssl" (implicit TLS, port 465), "starttls" or "none"
    pub secure: String,
}

impl SmtpConfig {
    /// None when SMTP_HOST or SMTP_FROM_EMAIL is not set: emails are disabled.
    pub fn from_env() -> Option<Self> {
        let var = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let host = var("SMTP_HOST")?;
        let from_email = var("SMTP_FROM_EMAIL")?;
        let port: u16 = var("SMTP_PORT").and_then(|p| p.parse().ok()).unwrap_or(587);
        let secure = var("SMTP_SECURE")
            .map(|s| s.to_lowercase())
            .filter(|s| ["ssl", "starttls", "none"].contains(&s.as_str()))
            .unwrap_or_else(|| if port == 465 { "ssl".into() } else { "starttls".into() });
        Some(SmtpConfig {
            host,
            port,
            username: var("SMTP_USERNAME").unwrap_or_default(),
            password: var("SMTP_PASSWORD").unwrap_or_default(),
            from_email,
            from_name: var("SMTP_FROM_NAME"),
            secure,
        })
    }

    fn transport(&self) -> ApiResult<AsyncSmtpTransport<Tokio1Executor>> {
        let err = |e: lettre::transport::smtp::Error| ApiError::internal(format!("SMTP configuration: {e}"));
        let builder = match self.secure.as_str() {
            "ssl" => AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host).map_err(err)?,
            "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host).map_err(err)?,
            _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.host),
        };
        let mut builder = builder.port(self.port).timeout(Some(Duration::from_secs(20)));
        if !self.username.is_empty() {
            builder = builder.credentials(Credentials::new(self.username.clone(), self.password.clone()));
        }
        Ok(builder.build())
    }

    pub async fn send(&self, to_email: &str, to_name: Option<&str>, mail: &Mail) -> ApiResult<()> {
        let bad = |e: String| ApiError::bad_request(format!("Cannot send the email: {e}"));
        let name = self.from_name.clone().unwrap_or_else(|| mail.sender.clone());
        let from = Mailbox::new(Some(name), self.from_email.parse().map_err(|e| bad(format!("{e}")))?);
        let to = Mailbox::new(to_name.map(|n| n.to_string()), to_email.parse().map_err(|e| bad(format!("{e}")))?);
        let message = Message::builder()
            .from(from)
            .to(to)
            .subject(&mail.subject)
            .multipart(MultiPart::alternative_plain_html(mail.text.clone(), mail.html.clone()))
            .map_err(|e| bad(e.to_string()))?;
        self.transport()?.send(message).await.map_err(|e| {
            tracing::warn!("email to {to_email} failed: {e}");
            ApiError::new(axum::http::StatusCode::BAD_GATEWAY, "email_failed", format!("The mail server refused the email: {e}"))
        })?;
        tracing::info!(to = to_email, subject = %mail.subject, "email sent");
        Ok(())
    }

    /// Connects and greets the server without sending anything.
    pub async fn check(&self) -> ApiResult<bool> {
        self.transport()?
            .test_connection()
            .await
            .map_err(|e| ApiError::new(axum::http::StatusCode::BAD_GATEWAY, "email_failed", format!("Cannot reach the mail server: {e}")))
    }
}

pub struct Mail {
    /// sender name when SMTP_FROM_NAME is not set
    pub sender: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Black or white text for a background colour
fn on(color: &str) -> &'static str {
    let n = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0x111827);
    let (r, g, b) = (((n >> 16) & 255) as f64, ((n >> 8) & 255) as f64, (n & 255) as f64);
    if 0.299 * r + 0.587 * g + 0.114 * b > 160.0 {
        "#111319"
    } else {
        "#ffffff"
    }
}

pub struct InviteMail<'a> {
    pub lang: &'a str,
    /// product name shown in the email (GetPaper or the custom one)
    pub product: &'a str,
    pub inviter: &'a str,
    pub team: &'a str,
    pub role: &'a str,
    pub email: &'a str,
    /// link that accepts the invite (or opens the workspace when already a member)
    pub link: &'a str,
    pub base_url: &'a str,
    /// the person already has an account and was added to the workspace directly
    pub added: bool,
    pub microsoft: bool,
    pub brand_color: &'a str,
}

/// The invitation email: a short, friendly message with one button.
pub fn invite(m: &InviteMail) -> Mail {
    let it = m.lang == "it";
    let role = match (m.role, it) {
        ("admin", true) => "amministratore",
        ("viewer", true) => "lettore",
        (_, true) => "editor",
        ("admin", false) => "admin",
        ("viewer", false) => "viewer",
        _ => "editor",
    };
    let role_hint = match (m.role, it) {
        ("admin", true) => "potrai leggere, scrivere e gestire il workspace",
        ("viewer", true) => "potrai consultare tutti i documenti",
        (_, true) => "potrai leggere, scrivere e organizzare i documenti",
        ("admin", false) => "you can read, write and manage the workspace",
        ("viewer", false) => "you can read every document",
        _ => "you can read, write and organise documents",
    };
    let (subject, title, intro, button, sign_in, fallback, ignore, footer) = if it {
        (
            if m.added { format!("{} ti ha aggiunto a {} su {}", m.inviter, m.team, m.product) } else { format!("{} ti ha invitato su {}", m.inviter, m.product) },
            if m.added { format!("Ora fai parte di {}", m.team) } else { format!("Sei stato invitato in {}", m.team) },
            format!(
                "<strong>{}</strong> {} al workspace <strong>{}</strong> su {}, la documentazione del team, come <strong>{role}</strong>: {role_hint}.",
                esc(m.inviter),
                if m.added { "ti ha aggiunto" } else { "ti ha invitato a unirti" },
                esc(m.team),
                esc(m.product)
            ),
            if m.added { format!("Apri {}", m.product) } else { "Accetta l'invito".to_string() },
            if m.microsoft {
                format!("Accedi con il tuo account Microsoft aziendale <strong>{}</strong>: non serve creare una password.", esc(m.email))
            } else {
                format!("Crea il tuo account con l'indirizzo <strong>{}</strong>.", esc(m.email))
            },
            "Se il pulsante non funziona, copia questo indirizzo nel browser:",
            "Se non aspettavi questo invito puoi ignorare questa email.",
            "Hai ricevuto questa email perché qualcuno ti ha invitato su",
        )
    } else {
        (
            if m.added { format!("{} added you to {} on {}", m.inviter, m.team, m.product) } else { format!("{} invited you to {}", m.inviter, m.product) },
            if m.added { format!("You are now part of {}", m.team) } else { format!("You're invited to {}", m.team) },
            format!(
                "<strong>{}</strong> {} the <strong>{}</strong> workspace on {}, your team's documentation, as <strong>{role}</strong>: {role_hint}.",
                esc(m.inviter),
                if m.added { "added you to" } else { "invited you to join" },
                esc(m.team),
                esc(m.product)
            ),
            if m.added { format!("Open {}", m.product) } else { "Accept invitation".to_string() },
            if m.microsoft {
                format!("Sign in with your work Microsoft account <strong>{}</strong>: no password needed.", esc(m.email))
            } else {
                format!("Create your account with <strong>{}</strong>.", esc(m.email))
            },
            "If the button doesn't work, paste this address in your browser:",
            "If you weren't expecting this invitation, you can ignore this email.",
            "You received this email because someone invited you to",
        )
    };
    let brand = m.brand_color;
    let brand_text = on(brand);
    let strip = |s: &str| {
        let mut out = String::new();
        let mut tag = false;
        for c in s.chars() {
            match c {
                '<' => tag = true,
                '>' => tag = false,
                c if !tag => out.push(c),
                _ => {}
            }
        }
        out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"")
    };
    let text = format!(
        "{title}\n\n{}\n\n{}\n\n{button}: {link}\n\n{ignore}\n-- \n{product} · {base}\n",
        strip(&intro),
        strip(&sign_in),
        link = m.link,
        base = m.base_url,
        product = m.product
    );
    let html = format!(
        r#"<!doctype html>
<html lang="{lang}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{subject_e}</title></head>
<body style="margin:0;padding:0;background:#f4f5f7;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;color:#111319;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#f4f5f7;padding:32px 12px;">
<tr><td align="center">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:520px;background:#ffffff;border-radius:12px;overflow:hidden;border:1px solid #e5e7eb;">
<tr><td style="background:{brand};padding:20px 32px;">
  <table role="presentation" cellpadding="0" cellspacing="0"><tr>
    <td style="width:30px;height:30px;background:{brand_text};border-radius:7px;padding:8px 8px 6px;" valign="top">
      <div style="height:3px;width:14px;background:{brand};border-radius:2px;margin:0 0 3px;font-size:0;line-height:0;">&nbsp;</div>
      <div style="height:3px;width:14px;background:{brand};border-radius:2px;margin:0 0 3px;font-size:0;line-height:0;">&nbsp;</div>
      <div style="height:3px;width:9px;background:{brand};border-radius:2px;font-size:0;line-height:0;">&nbsp;</div>
    </td>
    <td style="padding-left:10px;color:{brand_text};font-size:20px;font-weight:700;letter-spacing:-0.3px;">{product_e}</td>
  </tr></table>
</td></tr>
<tr><td style="padding:32px 32px 8px;">
  <h1 style="margin:0 0 16px;font-size:22px;line-height:1.3;">{title_e}</h1>
  <p style="margin:0 0 16px;font-size:15px;line-height:1.6;color:#374151;">{intro}</p>
  <p style="margin:0 0 28px;font-size:15px;line-height:1.6;color:#374151;">{sign_in}</p>
  <table role="presentation" cellpadding="0" cellspacing="0"><tr><td style="border-radius:8px;background:{brand};">
    <a href="{link_e}" style="display:inline-block;padding:12px 24px;font-size:15px;font-weight:600;color:{brand_text};text-decoration:none;border-radius:8px;">{button}</a>
  </td></tr></table>
  <p style="margin:28px 0 4px;font-size:13px;color:#6b7280;">{fallback}</p>
  <p style="margin:0 0 24px;font-size:13px;word-break:break-all;"><a href="{link_e}" style="color:#374151;">{link_e}</a></p>
</td></tr>
<tr><td style="padding:16px 32px 28px;border-top:1px solid #f0f1f3;">
  <p style="margin:0;font-size:12px;line-height:1.5;color:#9ca3af;">{ignore}<br>{footer} <a href="{base_e}" style="color:#9ca3af;">{base_e}</a>.</p>
</td></tr>
</table>
</td></tr></table>
</body></html>"#,
        lang = if it { "it" } else { "en" },
        subject_e = esc(&subject),
        title_e = esc(&title),
        link_e = esc(m.link),
        base_e = esc(m.base_url),
        product_e = esc(m.product),
    );
    Mail { sender: m.product.to_string(), subject, text, html }
}
