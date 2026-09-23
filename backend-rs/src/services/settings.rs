// Runtime-editable instance settings (community rules + bot defense mode).
//
// The admin edits these from the web panel (GET/POST /admin/settings) without
// restarting the server. Values live in the `settings` table as one jsonb
// scalar per key; the in-memory copy is a std RwLock on AppState::Inner and
// every consumer reads a cheap `settings_snapshot()` clone per request.
use sqlx::PgPool;

use crate::config::Config;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuntimeSettings {
    pub allow_multi_account: bool,
    pub allow_offensive_content: bool,
    pub allow_explicit_content: bool,
    pub allow_griefing: bool,
    pub allow_kind_griefing: bool,
    pub allow_political_griefing: bool,
    pub allow_vpn: bool,
    pub allow_bots: bool,
    pub extra_rules: Option<String>,
    /// "off" | "log" | "enforce"
    pub anti_bot_mode: String,
    pub anti_bot_enforce_threshold: i32,
}

/// Settings seeded from the env-driven config — the starting point before any
/// admin override has been stored.
pub fn defaults_from_config(config: &Config) -> RuntimeSettings {
    RuntimeSettings {
        allow_multi_account: config.allow_multi_account,
        allow_offensive_content: config.allow_offensive_content,
        allow_explicit_content: config.allow_explicit_content,
        allow_griefing: config.allow_griefing,
        allow_kind_griefing: config.allow_kind_griefing,
        allow_political_griefing: config.allow_political_griefing,
        allow_vpn: config.allow_vpn,
        allow_bots: config.allow_bots,
        extra_rules: config.extra_rules.clone(),
        anti_bot_mode: config.anti_bot_mode.clone(),
        anti_bot_enforce_threshold: config.anti_bot_enforce_threshold,
    }
}

/// Load settings from the DB, layered over the config defaults. Unknown keys
/// and malformed values are skipped (with a log line for the latter); a
/// missing/unreadable table falls back to the defaults.
pub async fn load(pool: &PgPool, config: &Config) -> RuntimeSettings {
    let mut settings = defaults_from_config(config);
    let rows: Vec<(String, serde_json::Value)> =
        match sqlx::query_as("SELECT key, value FROM settings")
            .fetch_all(pool)
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                eprintln!("[settings] failed to load, using config defaults: {err}");
                return settings;
            }
        };
    for (key, value) in rows {
        apply_key(&mut settings, &key, value);
    }
    settings
}

/// Apply one stored (key, jsonb scalar) onto the settings struct.
fn apply_key(settings: &mut RuntimeSettings, key: &str, value: serde_json::Value) {
    match key {
        "allow_multi_account" => set_bool(&mut settings.allow_multi_account, key, value),
        "allow_offensive_content" => set_bool(&mut settings.allow_offensive_content, key, value),
        "allow_explicit_content" => set_bool(&mut settings.allow_explicit_content, key, value),
        "allow_griefing" => set_bool(&mut settings.allow_griefing, key, value),
        "allow_kind_griefing" => set_bool(&mut settings.allow_kind_griefing, key, value),
        "allow_political_griefing" => set_bool(&mut settings.allow_political_griefing, key, value),
        "allow_vpn" => set_bool(&mut settings.allow_vpn, key, value),
        "allow_bots" => set_bool(&mut settings.allow_bots, key, value),
        "extra_rules" => match value {
            serde_json::Value::Null => settings.extra_rules = None,
            serde_json::Value::String(text) => settings.extra_rules = Some(text),
            _ => eprintln!("[settings] ignoring bad value for key '{key}'"),
        },
        "anti_bot_mode" => match value.as_str() {
            Some(mode) => settings.anti_bot_mode = mode.to_string(),
            None => eprintln!("[settings] ignoring bad value for key '{key}'"),
        },
        "anti_bot_enforce_threshold" => match value.as_i64().and_then(|n| i32::try_from(n).ok()) {
            Some(n) => settings.anti_bot_enforce_threshold = n,
            None => eprintln!("[settings] ignoring bad value for key '{key}'"),
        },
        // Unknown keys are ignored.
        _ => {}
    }
}

fn set_bool(field: &mut bool, key: &str, value: serde_json::Value) {
    match value.as_bool() {
        Some(v) => *field = v,
        None => eprintln!("[settings] ignoring bad value for key '{key}'"),
    }
}

/// Validate a FULL settings JSON object (as sent by the customize page):
/// all 8 booleans are required, extra_rules is a string (≤2000 chars) or
/// null, anti_bot_mode ∈ {off, log, enforce} and the threshold is an int in
/// 1..=100000.
pub fn validate(v: &serde_json::Value) -> Result<RuntimeSettings, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "expected a JSON object".to_string())?;

    let bool_field = |name: &str| -> Result<bool, String> {
        match obj.get(name) {
            Some(serde_json::Value::Bool(b)) => Ok(*b),
            Some(_) => Err(format!("field {name} must be a boolean")),
            None => Err(format!("missing field {name}")),
        }
    };

    let extra_rules = match obj.get("extra_rules") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => {
            if text.chars().count() > 2000 {
                return Err("field extra_rules must be at most 2000 characters".to_string());
            }
            Some(text.clone())
        }
        Some(_) => return Err("field extra_rules must be a string or null".to_string()),
    };

    let anti_bot_mode = match obj.get("anti_bot_mode") {
        Some(mode @ serde_json::Value::String(_))
            if matches!(mode.as_str(), Some("off" | "log" | "enforce")) =>
        {
            mode.as_str().expect("checked above").to_string()
        }
        Some(_) => {
            return Err("field anti_bot_mode must be one of: off, log, enforce".to_string());
        }
        None => return Err("missing field anti_bot_mode".to_string()),
    };

    let anti_bot_enforce_threshold = match obj.get("anti_bot_enforce_threshold") {
        Some(n @ serde_json::Value::Number(_)) => {
            let raw = n
                .as_i64()
                .ok_or_else(|| "field anti_bot_enforce_threshold must be an integer".to_string())?;
            if !(1..=100_000).contains(&raw) {
                return Err(
                    "field anti_bot_enforce_threshold must be between 1 and 100000".to_string(),
                );
            }
            raw as i32
        }
        Some(_) => {
            return Err("field anti_bot_enforce_threshold must be an integer".to_string());
        }
        None => return Err("missing field anti_bot_enforce_threshold".to_string()),
    };

    Ok(RuntimeSettings {
        allow_multi_account: bool_field("allow_multi_account")?,
        allow_offensive_content: bool_field("allow_offensive_content")?,
        allow_explicit_content: bool_field("allow_explicit_content")?,
        allow_griefing: bool_field("allow_griefing")?,
        allow_kind_griefing: bool_field("allow_kind_griefing")?,
        allow_political_griefing: bool_field("allow_political_griefing")?,
        allow_vpn: bool_field("allow_vpn")?,
        allow_bots: bool_field("allow_bots")?,
        extra_rules,
        anti_bot_mode,
        anti_bot_enforce_threshold,
    })
}

/// Upsert every setting key (one jsonb value per key) in a single transaction.
pub async fn save(pool: &PgPool, s: &RuntimeSettings) -> sqlx::Result<()> {
    let entries: [(&str, serde_json::Value); 11] = [
        (
            "allow_multi_account",
            serde_json::json!(s.allow_multi_account),
        ),
        (
            "allow_offensive_content",
            serde_json::json!(s.allow_offensive_content),
        ),
        (
            "allow_explicit_content",
            serde_json::json!(s.allow_explicit_content),
        ),
        ("allow_griefing", serde_json::json!(s.allow_griefing)),
        (
            "allow_kind_griefing",
            serde_json::json!(s.allow_kind_griefing),
        ),
        (
            "allow_political_griefing",
            serde_json::json!(s.allow_political_griefing),
        ),
        ("allow_vpn", serde_json::json!(s.allow_vpn)),
        ("allow_bots", serde_json::json!(s.allow_bots)),
        ("extra_rules", serde_json::json!(s.extra_rules)),
        ("anti_bot_mode", serde_json::json!(s.anti_bot_mode)),
        (
            "anti_bot_enforce_threshold",
            serde_json::json!(s.anti_bot_enforce_threshold),
        ),
    ];
    let mut tx = pool.begin().await?;
    for (key, value) in entries {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ($1, $2) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn full_object(mode: &str) -> serde_json::Value {
        json!({
            "allow_multi_account": true,
            "allow_offensive_content": false,
            "allow_explicit_content": false,
            "allow_griefing": true,
            "allow_kind_griefing": false,
            "allow_political_griefing": false,
            "allow_vpn": true,
            "allow_bots": false,
            "extra_rules": null,
            "anti_bot_mode": mode,
            "anti_bot_enforce_threshold": 100,
        })
    }

    #[test]
    fn validate_accepts_full_object() {
        let parsed = validate(&full_object("log")).expect("valid full object");
        assert_eq!(parsed.anti_bot_mode, "log");
        assert!(parsed.allow_multi_account);
        assert!(parsed.allow_vpn);
        assert!(!parsed.allow_bots);
        assert_eq!(parsed.anti_bot_enforce_threshold, 100);
        assert_eq!(parsed.extra_rules, None);
    }

    #[test]
    fn validate_rejects_bad_mode() {
        assert!(validate(&full_object("banana")).is_err());
        assert!(validate(&full_object("")).is_err());
        assert!(validate(&full_object("OFF")).is_err());
    }

    #[test]
    fn validate_rejects_missing_bool() {
        let mut v = full_object("off");
        v.as_object_mut().expect("object").remove("allow_bots");
        assert_eq!(validate(&v).unwrap_err(), "missing field allow_bots");
    }

    #[test]
    fn validate_rejects_bad_threshold_and_rules() {
        let mut v = full_object("enforce");
        v["anti_bot_enforce_threshold"] = json!(0);
        assert!(validate(&v).is_err());
        v["anti_bot_enforce_threshold"] = json!(100_001);
        assert!(validate(&v).is_err());
        v["anti_bot_enforce_threshold"] = json!(100_000);
        assert!(validate(&v).is_ok());
        v["extra_rules"] = json!("x".repeat(2001));
        assert!(validate(&v).is_err());
        v["extra_rules"] = serde_json::Value::Null;
        assert!(validate(&v).is_ok());
    }
}
