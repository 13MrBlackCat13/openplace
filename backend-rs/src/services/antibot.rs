// Anti-automation service — a fully self-hosted analogue of a fingerprinting
// SaaS (no third parties):
//   * paint behaviour analysis: machine-regular intervals, uniform batches,
//   * headless/automation UA markers,
//   * browser fingerprint collection (POST /fp/collect) keyed by an
//     HMAC-SHA256 visitor id over canonical traits,
//   * proof-of-work challenges (sha256(challenge:nonce) with N leading zero
//     bits) that clear the bot score for 24h when solved.
//
// Scores live in the `bot_flags` table; rolling paint windows, issued
// challenges and trust state are in-memory (single-process, like the rest of
// the backend).
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use dashmap::DashMap;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Rolling window cap: the last N paint calls per user are analysed.
const WINDOW_CAP: usize = 12;
/// Issued PoW challenges expire after this long.
const CHALLENGE_TTL: Duration = Duration::from_secs(10 * 60);
/// Solving a PoW marks the visitor trusted for this long.
const TRUST_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// bot_flags score cache TTL (pre_check is on the paint hot path).
const SCORE_CACHE_TTL: Duration = Duration::from_secs(30);
/// stddev/mean below this means machine-regular request intervals.
const REGULARITY_THRESHOLD: f64 = 0.08;

/// Roles exempt from scoring and enforcement.
const EXEMPT_ROLES: [&str; 3] = ["admin", "moderator", "global_moderator"];

/// UA substrings (lowercase) that indicate automation/headless clients.
const HEADLESS_UA_MARKERS: [&str; 7] = [
    "headlesschrome",
    "puppeteer",
    "playwright",
    "phantomjs",
    "python-requests",
    "curl/",
    "wget/",
];

#[derive(Debug, Clone)]
pub struct BotVerdict {
    pub score: i32,
    pub reasons: Vec<String>,
}

#[derive(Default)]
struct PaintWindow {
    /// (ts_ms, pixel count) of the last paints.
    samples: VecDeque<(i64, usize)>,
    /// Lifetime paint-call counter (for the fp_missing signal).
    total: u64,
    /// The visitor_accounts existence check is done once per process.
    fp_checked: bool,
}

pub struct AntibotStore {
    /// Per-user rolling paint window.
    windows: DashMap<i32, PaintWindow>,
    /// Issued PoW challenges: challenge hex → (owner user id, issued at).
    challenges: DashMap<String, (i32, Instant)>,
    /// Users that solved a PoW → trusted-until instant.
    trusted: DashMap<i32, Instant>,
    /// Score reasons already counted this session: (user_id, reason).
    flagged: DashMap<(i32, String), ()>,
    /// Cached bot score: user_id → (score, cached at).
    score_cache: DashMap<i32, (i32, Instant)>,
}

impl Default for AntibotStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AntibotStore {
    pub fn new() -> Self {
        Self {
            windows: DashMap::new(),
            challenges: DashMap::new(),
            trusted: DashMap::new(),
            flagged: DashMap::new(),
            score_cache: DashMap::new(),
        }
    }

    /// Owner of a still-valid challenge (10 min TTL).
    pub fn challenge_owner(&self, challenge: &str) -> Option<i32> {
        let entry = self.challenges.get(challenge)?;
        let (owner, issued_at) = *entry.value();
        (issued_at.elapsed() < CHALLENGE_TTL).then_some(owner)
    }

    /// Remove a challenge (after it was solved) and return its owner.
    pub fn take_challenge(&self, challenge: &str) -> Option<i32> {
        self.challenges
            .remove(challenge)
            .map(|(_, (owner, _))| owner)
    }

    /// Remaining trust time for a user that solved a PoW (None if untrusted).
    pub fn trusted_remaining(&self, user_id: i32) -> Option<Duration> {
        let issued = *self.trusted.get(&user_id)?.value();
        let elapsed = issued.elapsed();
        TRUST_TTL.checked_sub(elapsed)
    }

    /// Background TTL sweeper for challenges + trust entries.
    pub fn start_cleanup(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                self.challenges
                    .retain(|_, (_, at)| at.elapsed() < CHALLENGE_TTL);
                self.trusted.retain(|_, at| at.elapsed() < TRUST_TTL);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Public API (called from routes)
// ---------------------------------------------------------------------------

/// Paint pre-check. In enforce mode users whose bot score reached the
/// threshold are blocked (403) until they solve a PoW challenge.
pub async fn pre_check(state: &AppState, user_id: i32) -> ApiResult<()> {
    let settings = state.settings_snapshot();
    if settings.anti_bot_mode != "enforce" {
        return Ok(());
    }
    if let Some(runtime) = state.users.get_or_load(&state.pool, user_id).await {
        if EXEMPT_ROLES.contains(&runtime.role.as_str()) {
            return Ok(());
        }
    }
    if state.antibot.trusted_remaining(user_id).is_some() {
        return Ok(());
    }
    let score = score(state, user_id).await;
    if score >= settings.anti_bot_enforce_threshold {
        return Err(ApiError::forbidden("Forbidden"));
    }
    Ok(())
}

/// Record a successful paint and update the bot score. Fire-and-forget: the
/// analysis runs on a spawned task so the paint response is not delayed.
pub fn observe_paint(state: &AppState, user_id: i32, pixel_count: usize, user_agent: Option<&str>) {
    if state.settings_snapshot().anti_bot_mode == "off" {
        return;
    }
    let state = state.clone();
    let ua = user_agent.map(str::to_string);
    tokio::spawn(async move {
        observe_inner(&state, user_id, pixel_count, ua.as_deref()).await;
    });
}

/// Stable visitor id: first 32 hex chars of HMAC-SHA256(key, canonical).
pub fn visitor_id(key: &str, canonical: &str) -> String {
    let mac = hmac_sha256(key.as_bytes(), canonical.as_bytes());
    hex_encode(&mac)[..32].to_string()
}

/// Full sha256 hex of the canonical traits string.
pub fn sha256_hex(input: &str) -> String {
    hex_encode(&Sha256::digest(input.as_bytes()))
}

/// PoW check: sha256(challenge_hex + ":" + nonce_hex) must have at least
/// `bits` leading zero bits.
pub fn pow_valid(challenge_hex: &str, nonce_hex: &str, bits: u8) -> bool {
    if bits == 0 {
        return true;
    }
    if challenge_hex.is_empty()
        || nonce_hex.is_empty()
        || !is_hex(challenge_hex)
        || !is_hex(nonce_hex)
    {
        return false;
    }
    let digest = Sha256::digest(format!("{challenge_hex}:{nonce_hex}").as_bytes());
    leading_zero_bits(&digest) >= u32::from(bits)
}

/// Canonical traits string: sorted "k=v" pairs joined with ";" over the
/// string/number primitives of the traits object.
pub fn canonical_traits(traits: &serde_json::Value) -> String {
    let Some(obj) = traits.as_object() else {
        return String::new();
    };
    let mut pairs: Vec<String> = obj
        .iter()
        .filter_map(|(k, v)| {
            let value = match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => return None,
            };
            Some(format!("{k}={value}"))
        })
        .collect();
    pairs.sort();
    pairs.join(";")
}

/// Coarse headless/automation UA detection (case-insensitive markers).
pub fn is_headless_ua(user_agent: &str) -> bool {
    let ua = user_agent.to_ascii_lowercase();
    HEADLESS_UA_MARKERS.iter().any(|m| ua.contains(m))
}

/// Mint a 32-byte-hex PoW challenge bound to a user.
pub fn issue_challenge(store: &AntibotStore, user_id: i32) -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let challenge = hex_encode(&bytes);
    store
        .challenges
        .insert(challenge.clone(), (user_id, Instant::now()));
    challenge
}

/// Clear the bot score after a solved PoW: flags removed, visitor trusted
/// for 24h, rolling window reset.
pub async fn solve_pow(state: &AppState, user_id: i32) {
    let store = &state.antibot;
    let _ = sqlx::query("DELETE FROM bot_flags WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.pool)
        .await;
    store.trusted.insert(user_id, Instant::now());
    store.score_cache.remove(&user_id);
    store.windows.remove(&user_id);
    store.flagged.retain(|(uid, _), _| *uid != user_id);
    eprintln!("[antibot] user {user_id} solved PoW — flags cleared, trusted for 24h");
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

async fn observe_inner(
    state: &AppState,
    user_id: i32,
    pixel_count: usize,
    user_agent: Option<&str>,
) {
    // Staff is never scored; unknown users are skipped too.
    let Some(runtime) = state.users.get_or_load(&state.pool, user_id).await else {
        return;
    };
    if EXEMPT_ROLES.contains(&runtime.role.as_str()) {
        return;
    }

    if let Some(ua) = user_agent {
        if is_headless_ua(ua) {
            add_flag(state, user_id, "headless_ua", 60).await;
        }
    }

    let store = &state.antibot;
    let now_ms = Utc::now().timestamp_millis();
    let max_batch = runtime.max_charges.floor().max(0.0) as usize;

    let (deltas, uniform, total, fp_checked) = {
        let mut window = store.windows.entry(user_id).or_default();
        window.total += 1;
        window.samples.push_back((now_ms, pixel_count));
        while window.samples.len() > WINDOW_CAP {
            window.samples.pop_front();
        }
        let deltas: Vec<i64> = window
            .samples
            .iter()
            .zip(window.samples.iter().skip(1))
            .map(|(a, b)| b.0 - a.0)
            .collect();
        let uniform = window.samples.len() >= 6
            && window
                .samples
                .iter()
                .all(|(_, count)| *count == pixel_count);
        (deltas, uniform, window.total, window.fp_checked)
    };

    // Machine-regular request intervals (humans jitter).
    if deltas.len() >= 5 && regularity_ratio(&deltas) < REGULARITY_THRESHOLD {
        add_flag(state, user_id, "regular_intervals", 40).await;
    }

    // Every batch exactly at the user's max batch size — scripted pattern.
    if uniform && max_batch > 10 && pixel_count == max_batch {
        add_flag(state, user_id, "uniform_batches", 25).await;
    }

    // Many paints without a fingerprint link (lazy, once per process).
    if total >= 20 && !fp_checked {
        if let Some(mut window) = store.windows.get_mut(&user_id) {
            window.fp_checked = true;
        }
        let linked: Option<(i32,)> =
            sqlx::query_as("SELECT 1 FROM visitor_accounts WHERE user_id = $1 LIMIT 1")
                .bind(user_id)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();
        if linked.is_none() {
            add_flag(state, user_id, "fp_missing", 20).await;
        }
    }
}

/// Record one score reason: at most once per reason per session, never a
/// duplicate entry in the DB reasons array. Returns the new score.
async fn add_flag(state: &AppState, user_id: i32, reason: &str, weight: i32) {
    let store = &state.antibot;
    if store
        .flagged
        .insert((user_id, reason.to_string()), ())
        .is_some()
    {
        return;
    }
    // The DO UPDATE ... WHERE guard makes this a no-op when the reason is
    // already on record (e.g. after a restart).
    let new_score: Option<(i32,)> = sqlx::query_as(
        "INSERT INTO bot_flags (user_id, score, reasons) VALUES ($1, $2, ARRAY[$3]) \
         ON CONFLICT (user_id) DO UPDATE SET \
             score = bot_flags.score + $2, \
             reasons = bot_flags.reasons || ARRAY[$3], \
             updated_at = now() \
         WHERE NOT ($3 = ANY(bot_flags.reasons)) \
         RETURNING score",
    )
    .bind(user_id)
    .bind(weight)
    .bind(reason)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    if let Some((score,)) = new_score {
        store.score_cache.insert(user_id, (score, Instant::now()));
        if state.settings_snapshot().anti_bot_mode != "off" {
            eprintln!("[antibot] user {user_id} flagged: {reason} (+{weight}) score={score}");
        }
    }
}

/// bot_flags.score with a short in-memory cache (paint hot path).
pub async fn score(state: &AppState, user_id: i32) -> i32 {
    let store = &state.antibot;
    if let Some((score, at)) = store.score_cache.get(&user_id).map(|e| *e.value()) {
        if at.elapsed() < SCORE_CACHE_TTL {
            return score;
        }
    }
    let score: i32 = sqlx::query_as("SELECT score FROM bot_flags WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .map(|(s,)| s)
        .unwrap_or(0);
    store.score_cache.insert(user_id, (score, Instant::now()));
    score
}

/// Coefficient of variation (stddev/mean, population stddev) of the request
/// deltas. Small values ⇒ machine-regular timing. Returns INFINITY when the
/// mean is not positive (nothing regular to flag).
fn regularity_ratio(deltas_ms: &[i64]) -> f64 {
    if deltas_ms.is_empty() {
        return f64::INFINITY;
    }
    let n = deltas_ms.len() as f64;
    let mean = deltas_ms.iter().map(|&d| d as f64).sum::<f64>() / n;
    if mean <= 0.0 {
        return f64::INFINITY;
    }
    let variance = deltas_ms
        .iter()
        .map(|&d| {
            let x = d as f64 - mean;
            x * x
        })
        .sum::<f64>()
        / n;
    variance.sqrt() / mean
}

fn leading_zero_bits(digest: &[u8]) -> u32 {
    let mut bits = 0u32;
    for &byte in digest {
        if byte == 0 {
            bits += 8;
        } else {
            bits += byte.leading_zeros();
            break;
        }
    }
    bits
}

fn is_hex(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || (b'A'..=b'F').contains(&b))
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// HMAC-SHA256 (RFC 2104) — avoids pulling in an extra crate for one MAC.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut padded = [0u8; BLOCK];
    if key.len() > BLOCK {
        padded[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        padded[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= padded[i];
        opad[i] ^= padded[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner.finalize());
    outer.finalize().into()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visitor_id_is_stable() {
        let first = visitor_id("secret-key", "platform=Win32;ua=Firefox");
        let second = visitor_id("secret-key", "platform=Win32;ua=Firefox");
        assert_eq!(first, second);
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn visitor_id_differs_by_input_and_key() {
        assert_ne!(visitor_id("key", "a=1"), visitor_id("key", "a=2"));
        assert_ne!(visitor_id("key-one", "a=1"), visitor_id("key-two", "a=1"));
    }

    #[test]
    fn pow_valid_accepts_mined_nonce() {
        let challenge = "00112233445566778899aabbccddeeff";
        let mut nonce: u64 = 0;
        loop {
            let candidate = format!("{nonce:x}");
            let digest = Sha256::digest(format!("{challenge}:{candidate}").as_bytes());
            if leading_zero_bits(&digest) >= 8 {
                break;
            }
            nonce += 1;
        }
        assert!(pow_valid(challenge, &format!("{nonce:x}"), 8));
        // A nonce that almost surely fails 8 leading zero bits.
        assert!(!pow_valid(
            challenge,
            &format!("{:x}", nonce + 1_000_003),
            8
        ));
    }

    #[test]
    fn pow_valid_rejects_garbage() {
        assert!(!pow_valid("", "ff", 8));
        assert!(!pow_valid("ff", "", 8));
        assert!(!pow_valid("ff", "zz", 8));
        assert!(pow_valid("anything", "anything", 0));
    }

    #[test]
    fn regular_intervals_flagged() {
        assert!(regularity_ratio(&[1000; 6]) < 0.08);
    }

    #[test]
    fn human_jitter_not_flagged() {
        let deltas = [800, 1200, 950, 1100, 900, 1150];
        assert!(regularity_ratio(&deltas) > 0.08);
    }

    #[test]
    fn hmac_matches_reference_vector() {
        // RFC 4231 test case 2: HMAC-SHA256("Jefe", "what do ya want for nothing?")
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex_encode(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn canonical_traits_sorts_and_skips_non_primitives() {
        let traits = serde_json::json!({ "b": 2, "a": "x", "obj": { "n": 1 } });
        assert_eq!(canonical_traits(&traits), "a=x;b=2");
        assert_eq!(canonical_traits(&serde_json::Value::Null), "");
    }
}
