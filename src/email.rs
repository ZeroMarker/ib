//! Minimal Resend adapter for email verification.
//!
//! Configuration comes from the environment so deployments only need to
//! set values in `/etc/ib/ib.env`:
//!
//! - `RESEND_API_KEY`: required to actually send. Empty/missing means
//!   verification emails are skipped and `email_verified` stays `false`.
//! - `RESEND_FROM`: sender identity, e.g. `ib <verify@example.com>`.
//!   Defaults to `ib <onboarding@resend.dev>` for Resend sandbox testing.
//! - `APP_BASE_URL`: public origin used to build the verification link,
//!   e.g. `https://ibkr.20070809.xyz`. Defaults to
//!   `http://127.0.0.1:8081`.

use std::env;

const RESEND_ENDPOINT: &str = "https://api.resend.com/emails";

fn api_key() -> Option<String> {
    env::var("RESEND_API_KEY")
        .ok()
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
}

/// True when a real send can be attempted.
pub fn is_configured() -> bool {
    api_key().is_some()
}

fn from_addr() -> String {
    env::var("RESEND_FROM")
        .ok()
        .map(|from| from.trim().to_owned())
        .filter(|from| !from.is_empty())
        .unwrap_or_else(|| "ib <onboarding@resend.dev>".to_string())
}

fn base_url() -> String {
    env::var("APP_BASE_URL")
        .ok()
        .map(|url| url.trim().trim_end_matches('/').to_owned())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:8081".to_string())
}

/// Verification link mailed to the user. The frontend reads
/// `?verify_token=` on boot and calls `POST /api/auth/verify`.
pub fn verification_link(token: &str) -> String {
    format!("{}/?verify_token={token}", base_url())
}

fn verification_html(link: &str, token: &str) -> String {
    format!(
        "<p>欢迎来到 ib 模拟交易平台。请在 24 小时内验证你的邮箱：</p>\
         <p><a href=\"{link}\">验证邮箱</a></p>\
         <p>如果按钮无法点击，把下面这串验证码粘贴到页面验证横幅中：</p>\
         <p><code>{token}</code></p>\
         <p>这是模拟交易服务，不会发送真实订单。</p>"
    )
}

fn verification_text(link: &str, token: &str) -> String {
    format!(
        "欢迎来到 ib 模拟交易平台。请在 24 小时内验证你的邮箱：\n{link}\n\n\
         如果链接无法点击，把这串验证码粘贴到页面验证横幅中：\n{token}\n"
    )
}

/// Send a verification email. Returns `Err` when unconfigured or when
/// Resend rejects the request; callers log the failure and keep the
/// account unverified instead of failing registration.
pub async fn send_verification(to: &str, token: &str) -> Result<(), String> {
    let key = api_key().ok_or_else(|| "RESEND_API_KEY is not configured".to_string())?;
    let link = verification_link(token);
    let payload = serde_json::json!({
        "from": from_addr(),
        "to": [to],
        "subject": "验证你的 ib 模拟交易邮箱",
        "html": verification_html(&link, token),
        "text": verification_text(&link, token),
    });
    let client = reqwest::Client::new();
    let response = client
        .post(RESEND_ENDPOINT)
        .bearer_auth(key)
        .json(&payload)
        .send()
        .await
        .map_err(|error| format!("resend request failed: {error}"))?;
    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        // Truncate provider bodies: they can echo the payload back.
        let body: String = body.chars().take(300).collect();
        Err(format!("resend rejected email: {status} {body}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_link_appends_token() {
        // Base URL comes from the environment; restore it afterwards so
        // the test does not leak state into other tests.
        let previous = env::var("APP_BASE_URL").ok();
        env::set_var("APP_BASE_URL", "https://example.com/ibkr/");
        assert_eq!(
            verification_link("abc123"),
            "https://example.com/ibkr/?verify_token=abc123"
        );
        match previous {
            Some(value) => env::set_var("APP_BASE_URL", value),
            None => env::remove_var("APP_BASE_URL"),
        }
    }

    #[test]
    fn verification_bodies_carry_link_and_token() {
        let html = verification_html("https://example.com/?verify_token=t", "t");
        let text = verification_text("https://example.com/?verify_token=t", "t");
        assert!(html.contains("https://example.com/?verify_token=t"));
        assert!(html.contains("<code>t</code>"));
        assert!(text.contains('t'));
    }
}
