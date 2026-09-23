use chrono::Utc;
use dashmap::DashMap;

/// In-memory sliding-window rate limiter, port of src/services/rate-limiter.ts.
/// Single-process by design (the JS backend is single-process too).
pub struct RateLimiter {
    map: DashMap<String, Entry>,
}

#[derive(Clone)]
struct Entry {
    count: i64,
    first_attempt: i64,
    last_attempt: i64,
    blocked: bool,
    block_until: i64,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    /// Returns Some(reset_time_ms) when denied.
    pub fn check(&self, key: &str, max_attempts: i64, window_ms: i64) -> Option<i64> {
        let now = Utc::now().timestamp_millis();
        let mut entry = self.map.entry(key.to_string()).or_insert(Entry {
            count: 0,
            first_attempt: now,
            last_attempt: now,
            blocked: false,
            block_until: 0,
        });
        if entry.blocked {
            if now < entry.block_until {
                return Some(entry.block_until);
            }
            *entry = Entry {
                count: 0,
                first_attempt: now,
                last_attempt: now,
                blocked: false,
                block_until: 0,
            };
        }
        if now - entry.first_attempt > window_ms {
            entry.count = 0;
            entry.first_attempt = now;
        }
        entry.count += 1;
        entry.last_attempt = now;
        if entry.count > max_attempts {
            entry.blocked = true;
            entry.block_until = now + window_ms * 2;
            return Some(entry.block_until);
        }
        None
    }

    pub fn record_success(&self, key: &str) {
        if let Some(mut entry) = self.map.get_mut(key) {
            entry.count = (entry.count - 1).max(0);
        }
    }

    pub fn start_cleanup(self: std::sync::Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                let now = Utc::now().timestamp_millis();
                self.map.retain(|_, e| now - e.last_attempt <= 300_000);
            }
        });
    }
}
