// Discord gateway bot — port of src/discord/bot.ts with discord.js replaced
// by a minimal REST + raw WebSocket gateway client (intents: GUILDS |
// GUILD_MEMBERS). Reconnects every 5s on error/close; resyncs all linked
// users on every READY.
use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, Stream, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::error::ApiResult;
use crate::state::AppState;

const API: &str = "https://discord.com/api/v10";
const USER_AGENT: &str = "openplace (https://openplace.live, 1.0.0)";
/// GatewayIntentBits.Guilds (1) | GatewayIntentBits.GuildMembers (1 << 1).
const GATEWAY_INTENTS: i64 = 1 | (1 << 1);
const MEMBERS_PAGE: usize = 1000;

type WsError = tokio_tungstenite::tungstenite::Error;

/// Spawns the gateway loop as a background task; missing token = disabled.
pub async fn start(state: AppState) {
    let Some(token) = state.config.discord_bot_token.clone() else {
        println!("[Discord Bot] Not configured");
        return;
    };
    if state.config.discord_server_id.is_none() {
        println!("[Discord Bot] Not configured (DISCORD_SERVER_ID is missing)");
        return;
    }
    tokio::spawn(async move {
        loop {
            if let Err(err) = run_session(&state, &token).await {
                eprintln!("[Discord Bot] Session error: {err}");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn run_session(state: &AppState, token: &str) -> Result<(), String> {
    let server_id = state
        .config
        .discord_server_id
        .clone()
        .ok_or_else(|| "server id not set".to_string())?;

    let url = gateway_url(state, token).await?;
    let (ws, _) = connect_async(url.as_str())
        .await
        .map_err(|e| format!("gateway connect: {e}"))?;
    println!("[Discord Bot] Connected to gateway");

    let (mut write, mut read) = ws.split();

    // HELLO (op 10) → heartbeat_interval.
    let hello: Value = serde_json::from_str(&next_text(&mut read).await?)
        .map_err(|e| format!("bad HELLO payload: {e}"))?;
    let heartbeat_ms = hello
        .pointer("/d/heartbeat_interval")
        .and_then(Value::as_i64)
        .unwrap_or(41_250)
        .max(1_000) as u64;

    // IDENTIFY (op 2).
    let identify = json!({
        "op": 2,
        "d": {
            "token": token,
            "intents": GATEWAY_INTENTS,
            "properties": {
                "os": "windows",
                "browser": "openplace",
                "device": "openplace",
            },
        },
    });
    write
        .send(Message::Text(identify.to_string()))
        .await
        .map_err(|e| format!("send IDENTIFY: {e}"))?;

    let mut seq: Option<i64> = None;
    let mut next_beat = tokio::time::Instant::now() + Duration::from_millis(heartbeat_ms);
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(next_beat) => {
                let payload = json!({ "op": 1, "d": seq });
                write
                    .send(Message::Text(payload.to_string()))
                    .await
                    .map_err(|e| format!("send heartbeat: {e}"))?;
                next_beat = tokio::time::Instant::now() + Duration::from_millis(heartbeat_ms);
            }
            incoming = read.next() => {
                let Some(msg) = incoming else {
                    return Err("gateway closed".to_string());
                };
                let msg = msg.map_err(|e| format!("recv: {e}"))?;
                let Message::Text(text) = msg else { continue };
                let payload: Value =
                    serde_json::from_str(&text).map_err(|e| format!("bad payload: {e}"))?;
                if let Some(s) = payload.get("s").and_then(Value::as_i64) {
                    seq = Some(s);
                }
                match payload.get("op").and_then(Value::as_i64) {
                    Some(0) => {
                        let event = payload.get("t").and_then(Value::as_str).unwrap_or_default();
                        let d = payload.get("d").cloned().unwrap_or(Value::Null);
                        match event {
                            "READY" => {
                                println!("[Discord Bot] Ready");
                                let st = state.clone();
                                let sid = server_id.clone();
                                tokio::spawn(async move {
                                    sync_all(&st, &sid).await;
                                });
                            }
                            "GUILD_MEMBER_UPDATE" => {
                                handle_member_event(state, &server_id, &d, false).await;
                            }
                            "GUILD_MEMBER_REMOVE" => {
                                handle_member_event(state, &server_id, &d, true).await;
                            }
                            _ => {}
                        }
                    }
                    // Reconnect / invalid session → drop and reconnect.
                    Some(7) | Some(9) => {
                        return Err(format!("server requested reconnect (op {})", payload.get("op").and_then(Value::as_i64).unwrap_or_default()));
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn next_text<S>(read: &mut S) -> Result<String, String>
where
    S: Stream<Item = Result<Message, WsError>> + Unpin,
{
    loop {
        let Some(msg) = read.next().await else {
            return Err("gateway closed".to_string());
        };
        match msg.map_err(|e| e.to_string())? {
            Message::Text(text) => return Ok(text),
            Message::Close(frame) => return Err(format!("gateway closed: {frame:?}")),
            _ => {}
        }
    }
}

async fn gateway_url(state: &AppState, token: &str) -> Result<String, String> {
    let resp = state
        .http
        .get(format!("{API}/gateway/bot"))
        .header("Authorization", format!("Bot {token}"))
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("gateway fetch: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("gateway fetch failed: {}", resp.status()));
    }
    let payload: Value = resp.json().await.map_err(|e| e.to_string())?;
    payload
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "gateway url missing".to_string())
}

/// GUILD_MEMBER_UPDATE / GUILD_MEMBER_REMOVE dispatch handler.
async fn handle_member_event(state: &AppState, server_id: &str, d: &Value, removed: bool) {
    if let Some(guild_id) = d.get("guild_id").and_then(Value::as_str) {
        if guild_id != server_id {
            return;
        }
    }
    let Some(discord_user_id) = d.pointer("/user/id").and_then(Value::as_str) else {
        return;
    };
    let roles: Option<Vec<String>> = if removed {
        None
    } else {
        d.get("roles").and_then(Value::as_array).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
    };
    if let Err(err) = apply_cooldown(state, discord_user_id, roles.as_deref()).await {
        eprintln!("[Discord Bot] Error handling role change: {err}");
    }
}

/// Role → cooldown mapping (mirrors DiscordBot.updateUser): base unless
/// COOLDOWN_OVERRIDE_FOR_SPECIAL, then active, booster (wins), or special.
fn cooldown_for(config: &crate::config::Config, roles: Option<&[String]>) -> i64 {
    if config.cooldown_override_for_special {
        return config.special_cooldown_ms;
    }
    let mut cooldown = config.cooldown_ms;
    if let Some(roles) = roles {
        if roles
            .iter()
            .any(|r| config.discord_active_role_ids.iter().any(|a| a == r))
        {
            cooldown = config.active_cooldown_ms;
        }
        if roles
            .iter()
            .any(|r| config.discord_booster_role_ids.iter().any(|b| b == r))
        {
            cooldown = config.booster_cooldown_ms;
        }
    }
    cooldown
}

/// Updates charges_cooldown_ms for the linked user when it differs, mirrors
/// DiscordBot.updateCooldown (+ runtime cache invalidation).
async fn apply_cooldown(
    state: &AppState,
    discord_user_id: &str,
    roles: Option<&[String]>,
) -> Result<(), String> {
    let Some((id, name, _discord_id, current)) = sqlx::query_as::<
        _,
        (i32, String, String, i32),
    >(
        "SELECT id, name, discord_user_id, charges_cooldown_ms FROM users WHERE discord_user_id = $1",
    )
    .bind(discord_user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };

    let cooldown = cooldown_for(&state.config, roles);
    if current == cooldown as i32 {
        return Ok(());
    }
    sqlx::query("UPDATE users SET charges_cooldown_ms = $2 WHERE id = $1")
        .bind(id)
        .bind(cooldown)
        .execute(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    state.users.invalidate(id);
    println!(
        "[Discord Bot] {name}#{id} (discord id {discord_user_id}) updated with cooldown {cooldown}ms"
    );
    Ok(())
}

/// Full member sync (mirror of syncAllUsers): REST pagination over guild
/// members, updating every linked user found.
async fn sync_all(state: &AppState, server_id: &str) {
    let Some(token) = state.config.discord_bot_token.clone() else {
        return;
    };
    let linked: Vec<(i32, String, String, i32)> = match sqlx::query_as(
        "SELECT id, name, discord_user_id, charges_cooldown_ms FROM users WHERE discord_user_id IS NOT NULL",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            eprintln!("[Discord Bot] Error during sync: {err}");
            return;
        }
    };
    let mut by_discord_id: HashMap<String, (i32, String, i32)> = HashMap::new();
    for (id, name, discord_user_id, cooldown) in linked {
        by_discord_id.insert(discord_user_id, (id, name, cooldown));
    }

    let mut after = String::from("0");
    let mut scanned = 0usize;
    loop {
        let url = format!("{API}/guilds/{server_id}/members?limit={MEMBERS_PAGE}&after={after}");
        let members: Vec<Value> = match state
            .http
            .get(&url)
            .header("Authorization", format!("Bot {token}"))
            .header("User-Agent", USER_AGENT)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => match resp.json().await {
                Ok(members) => members,
                Err(err) => {
                    eprintln!("[Discord Bot] Error during sync: {err}");
                    return;
                }
            },
            Ok(resp) => {
                eprintln!(
                    "[Discord Bot] Error during sync: members fetch failed with {}",
                    resp.status()
                );
                return;
            }
            Err(err) => {
                eprintln!("[Discord Bot] Error during sync: {err}");
                return;
            }
        };

        if members.is_empty() {
            break;
        }
        scanned += members.len();

        for member in &members {
            let Some(discord_user_id) = member.pointer("/user/id").and_then(Value::as_str) else {
                continue;
            };
            let Some((id, name, current)) = by_discord_id.get(discord_user_id) else {
                continue;
            };
            let roles: Option<Vec<String>> =
                member.get("roles").and_then(Value::as_array).map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                });
            let cooldown = cooldown_for(&state.config, roles.as_deref());
            if *current == cooldown as i32 {
                continue;
            }
            match sqlx::query("UPDATE users SET charges_cooldown_ms = $2 WHERE id = $1")
                .bind(*id)
                .bind(cooldown)
                .execute(&state.pool)
                .await
            {
                Ok(_) => {
                    state.users.invalidate(*id);
                    println!(
                        "[Discord Bot] {name}#{id} (discord id {discord_user_id}) updated with cooldown {cooldown}ms"
                    );
                }
                Err(err) => {
                    eprintln!("[Discord Bot] Error syncing user {name}#{id}: {err}");
                }
            }
        }

        let Some(last_id) = members
            .last()
            .and_then(|m| m.pointer("/user/id"))
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            break;
        };
        if members.len() < MEMBERS_PAGE {
            break;
        }
        after = last_id;
    }
    println!("[Discord Bot] Loaded {scanned} members");
}

/// Mirror of DiscordBot.updateUserId: after a successful link, refresh the
/// cooldown from the member's current roles. Best-effort.
pub async fn refresh_user_roles(state: &AppState, discord_user_id: &str) -> Result<(), String> {
    let Some(token) = state.config.discord_bot_token.clone() else {
        return Ok(());
    };
    let Some(server_id) = state.config.discord_server_id.clone() else {
        return Ok(());
    };
    let resp = state
        .http
        .get(format!(
            "{API}/guilds/{server_id}/members/{discord_user_id}"
        ))
        .header("Authorization", format!("Bot {token}"))
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        // Not a guild member (yet) — same silent no-op as the JS port.
        return Ok(());
    }
    let member: Value = resp.json().await.map_err(|e| e.to_string())?;
    let roles: Option<Vec<String>> = member.get("roles").and_then(Value::as_array).map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    });
    apply_cooldown(state, discord_user_id, roles.as_deref()).await
}

/// Sends a DM embed to the linked Discord account of `user_id`. Best-effort:
/// all errors are logged, never propagated.
pub async fn send_dm(state: &AppState, user_id: i32, title: &str, message: &str) -> ApiResult<()> {
    let Some(token) = state.config.discord_bot_token.clone() else {
        return Ok(());
    };
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT discord_user_id FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?;
    let Some(Some(discord_user_id)) = row.map(|r| r.0) else {
        return Ok(());
    };

    let create = match state
        .http
        .post(format!("{API}/users/@me/channels"))
        .header("Authorization", format!("Bot {token}"))
        .header("User-Agent", USER_AGENT)
        .json(&json!({ "recipient_id": discord_user_id }))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            eprintln!("[Discord Bot] Error sending DM: {err}");
            return Ok(());
        }
    };
    if !create.status().is_success() {
        eprintln!(
            "[Discord Bot] Error sending DM: channel create failed with {}",
            create.status()
        );
        return Ok(());
    }
    let channel: Value = create.json().await.unwrap_or(Value::Null);
    let Some(channel_id) = channel.get("id").and_then(Value::as_str) else {
        eprintln!("[Discord Bot] Error sending DM: no channel id in response");
        return Ok(());
    };

    let payload = json!({
        "embeds": [{
            "title": title,
            "description": message,
            "color": 0x41_69_E2,
            "author": {
                "name": "openplace",
                "icon_url": "https://openplace.live/img/favicon-96x96.png",
            },
        }],
    });
    match state
        .http
        .post(format!("{API}/channels/{channel_id}/messages"))
        .header("Authorization", format!("Bot {token}"))
        .header("User-Agent", USER_AGENT)
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) if !resp.status().is_success() => {
            eprintln!(
                "[Discord Bot] Error sending DM: message send failed with {}",
                resp.status()
            );
        }
        Err(err) => {
            eprintln!("[Discord Bot] Error sending DM: {err}");
        }
        _ => {}
    }
    Ok(())
}
