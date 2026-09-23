// Discord OAuth — port of src/services/discord.ts.
use serde::Deserialize;
use sqlx::PgPool;

use crate::config::Config;
use crate::services::user_cache::UserCache;

/// Minimal shape of `GET /users/@me` actually used here.
#[derive(Debug, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
}

/// Mirrors `DiscordService.isConfigured`.
pub fn is_configured(config: &Config) -> bool {
    config.discord_client_id.is_some()
        && config.discord_client_secret.is_some()
        && config.discord_redirect_url.is_some()
}

/// Mirrors `DiscordService.getAuthorizationUrl` (URLSearchParams ordering).
pub fn authorize_url(config: &Config, state: &str) -> String {
    let client_id = percent_encode(config.discord_client_id.as_deref().unwrap_or_default());
    let redirect_uri = percent_encode(config.discord_redirect_url.as_deref().unwrap_or_default());
    let state = percent_encode(state);
    format!(
        "https://discord.com/api/oauth2/authorize\
         ?client_id={client_id}&redirect_uri={redirect_uri}\
         &response_type=code&scope=identify&state={state}"
    )
}

/// Minimal percent-encoding: RFC 3986 unreserved bytes pass through, the rest
/// become `%XX` (matches what URLSearchParams/encodeURIComponent produce for
/// the values sent to Discord).
pub fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Token exchange + `GET /users/@me` — mirrors `exchangeCodeForToken` and
/// `getDiscordUser`.
pub async fn exchange_code(
    http: &reqwest::Client,
    config: &Config,
    code: &str,
) -> Result<DiscordUser, String> {
    let (Some(client_id), Some(client_secret), Some(redirect_uri)) = (
        config.discord_client_id.as_deref(),
        config.discord_client_secret.as_deref(),
        config.discord_redirect_url.as_deref(),
    ) else {
        return Err("Discord auth not configured".to_string());
    };

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
    }

    let token_resp = http
        .post("https://discord.com/api/oauth2/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| format!("Failed to exchange code for token: {e}"))?;
    if !token_resp.status().is_success() {
        return Err(format!(
            "Failed to exchange code for token: {}",
            token_resp.status()
        ));
    }
    let token: TokenResponse = token_resp
        .json()
        .await
        .map_err(|e| format!("Failed to exchange code for token: {e}"))?;

    let user_resp = http
        .get("https://discord.com/api/users/@me")
        .bearer_auth(&token.access_token)
        .send()
        .await
        .map_err(|e| format!("Failed to get Discord user: {e}"))?;
    if !user_resp.status().is_success() {
        return Err(format!(
            "Failed to get Discord user: {}",
            user_resp.status()
        ));
    }
    user_resp
        .json()
        .await
        .map_err(|e| format!("Failed to get Discord user: {e}"))
}

/// Mirrors `linkDiscordAccount`: reject if the Discord id is already bound to
/// another user, otherwise write the link and invalidate the runtime cache.
pub async fn link(
    pool: &PgPool,
    users_cache: &UserCache,
    user_id: i32,
    discord_user_id: &str,
    discord_username: &str,
) -> Result<(), String> {
    let taken: Option<(i32,)> =
        sqlx::query_as("SELECT id FROM users WHERE discord_user_id = $1 AND id <> $2")
            .bind(discord_user_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    if taken.is_some() {
        return Err("This Discord account is already linked to another user".to_string());
    }

    sqlx::query("UPDATE users SET discord_user_id = $2, discord = $3 WHERE id = $1")
        .bind(user_id)
        .bind(discord_user_id)
        .bind(discord_username)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    users_cache.invalidate(user_id);
    Ok(())
}
