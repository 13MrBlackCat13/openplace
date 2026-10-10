use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use dashmap::DashMap;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;

/// In-memory sliding-window rate limiter. Based on the port of
/// src/services/rate-limiter.ts, with two deliberate improvements: a true
/// sliding window (the original fixed window allowed a full-limit burst at
/// every window rollover) and cleanup that keeps entries with active blocks
/// (the original silently lifted a block after 5 minutes while the client
/// was still hammering). Single-process by design (the JS backend is
/// single-process too).
pub struct RateLimiter {
    map: DashMap<String, Entry>,
    now_fn: Arc<dyn Fn() -> i64 + Send + Sync>,
    cleanup_started: AtomicBool,
    cleanup_interval: Duration,
    idle_ttl_ms: i64,
    #[cfg(test)]
    cleanup_spawn_count: AtomicUsize,
}

#[derive(Clone)]
struct Entry {
    attempts: VecDeque<i64>,
    last_attempt: i64,
    block_until: Option<i64>,
}

impl Entry {
    fn new(now: i64) -> Self {
        Self {
            attempts: VecDeque::new(),
            last_attempt: now,
            block_until: None,
        }
    }

    fn prune_window(&mut self, now: i64, window_ms: i64) {
        while let Some(&attempt_at) = self.attempts.front() {
            if now.saturating_sub(attempt_at) >= window_ms {
                self.attempts.pop_front();
            } else {
                break;
            }
        }
    }
}

const DEFAULT_CLEANUP_INTERVAL: Duration = Duration::from_secs(60);
const DEFAULT_IDLE_TTL_MS: i64 = 5 * 60 * 1000;

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::with_clock_and_cleanup(
            Arc::new(|| Utc::now().timestamp_millis()),
            DEFAULT_CLEANUP_INTERVAL,
            DEFAULT_IDLE_TTL_MS,
        )
    }

    fn with_clock_and_cleanup(
        now_fn: Arc<dyn Fn() -> i64 + Send + Sync>,
        cleanup_interval: Duration,
        idle_ttl_ms: i64,
    ) -> Self {
        Self {
            map: DashMap::new(),
            now_fn,
            cleanup_started: AtomicBool::new(false),
            cleanup_interval,
            idle_ttl_ms,
            #[cfg(test)]
            cleanup_spawn_count: AtomicUsize::new(0),
        }
    }

    /// Returns Some(reset_time_ms) when denied.
    pub fn check(&self, key: &str, max_attempts: i64, window_ms: i64) -> Option<i64> {
        let now = (self.now_fn)();
        if max_attempts <= 0 || window_ms <= 0 {
            eprintln!(
                "[rate-limit] invalid configuration for key={key}: max_attempts={max_attempts}, window_ms={window_ms}"
            );
            return Some(now);
        }

        let mut entry = self
            .map
            .entry(key.to_string())
            .or_insert_with(|| Entry::new(now));

        if let Some(block_until) = entry.block_until {
            if now < block_until {
                return Some(block_until);
            }
            entry.block_until = None;
        }

        entry.prune_window(now, window_ms);

        if entry.attempts.len() as i64 >= max_attempts {
            let block_until = now.saturating_add(window_ms.saturating_mul(2));
            entry.block_until = Some(block_until);
            entry.last_attempt = now;
            return Some(block_until);
        }

        entry.attempts.push_back(now);
        entry.last_attempt = now;
        None
    }

    /// Marks one previously counted attempt as successful by removing one
    /// pending timestamp for this key (the most recent one), so successful
    /// operations do not accumulate toward the sliding-window limit.
    ///
    /// Failed attempts remain counted until they age out of each request's
    /// configured window. Active blocks are left unchanged.
    pub fn record_success(&self, key: &str) {
        if let Some(mut entry) = self.map.get_mut(key) {
            let now = (self.now_fn)();
            if entry.block_until.is_some_and(|until| now < until) {
                return;
            }
            let _ = entry.attempts.pop_back();
            entry.last_attempt = now;
        }
    }

    pub fn start_cleanup(self: std::sync::Arc<Self>) {
        if self.cleanup_started.swap(true, Ordering::AcqRel) {
            return;
        }
        #[cfg(test)]
        self.cleanup_spawn_count.fetch_add(1, Ordering::Relaxed);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(self.cleanup_interval).await;
                let now = (self.now_fn)();
                self.cleanup_at(now);
            }
        });
    }

    fn cleanup_at(&self, now: i64) {
        self.map.retain(|_, entry| {
            if entry.block_until.is_some_and(|until| now < until) {
                return true;
            }
            if entry.block_until.is_some_and(|until| now >= until) {
                entry.block_until = None;
            }
            now.saturating_sub(entry.last_attempt) <= self.idle_ttl_ms
        });
    }

    #[cfg(test)]
    fn cleanup_spawn_count(&self) -> usize {
        self.cleanup_spawn_count.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    #[derive(Clone)]
    struct TestClock {
        now_ms: Arc<AtomicI64>,
    }

    impl TestClock {
        fn new(start_ms: i64) -> Self {
            Self {
                now_ms: Arc::new(AtomicI64::new(start_ms)),
            }
        }

        fn set(&self, value: i64) {
            self.now_ms.store(value, Ordering::Relaxed);
        }

        fn now(&self) -> i64 {
            self.now_ms.load(Ordering::Relaxed)
        }
    }

    fn limiter_with_clock(clock: TestClock) -> RateLimiter {
        RateLimiter::with_clock_and_cleanup(
            Arc::new(move || clock.now()),
            Duration::from_millis(10),
            DEFAULT_IDLE_TTL_MS,
        )
    }

    #[test]
    fn uses_true_sliding_window_boundaries() {
        let clock = TestClock::new(0);
        let limiter = limiter_with_clock(clock.clone());

        assert_eq!(limiter.check("ip", 2, 100), None);
        clock.set(50);
        assert_eq!(limiter.check("ip", 2, 100), None);

        // At exactly +100ms, the first attempt is outside the sliding window.
        clock.set(100);
        assert_eq!(limiter.check("ip", 2, 100), None);

        clock.set(149);
        assert_eq!(limiter.check("ip", 2, 100), Some(349));
    }

    #[test]
    fn rejects_non_positive_limits_and_windows() {
        let clock = TestClock::new(1_000);
        let limiter = limiter_with_clock(clock.clone());

        assert_eq!(limiter.check("ip", 0, 100), Some(1_000));
        assert_eq!(limiter.check("ip", 1, 0), Some(1_000));
        assert_eq!(limiter.check("ip", -1, 100), Some(1_000));
        assert_eq!(limiter.check("ip", 1, -100), Some(1_000));
        assert!(limiter.map.get("ip").is_none());
    }

    #[test]
    fn cleanup_preserves_active_blocks_and_removes_expired_entries() {
        let clock = TestClock::new(0);
        let limiter = limiter_with_clock(clock.clone());

        assert_eq!(limiter.check("ip", 1, 200_000), None);
        assert_eq!(limiter.check("ip", 1, 200_000), Some(400_000));

        clock.set(350_000);
        limiter.cleanup_at(clock.now());
        let blocked_until = limiter
            .map
            .get("ip")
            .and_then(|entry| entry.block_until)
            .expect("active block must be preserved");
        assert_eq!(blocked_until, 400_000);

        clock.set(450_001);
        limiter.cleanup_at(clock.now());
        assert!(limiter.map.get("ip").is_none());
    }

    #[tokio::test]
    async fn start_cleanup_is_idempotent() {
        let clock = TestClock::new(0);
        let limiter = Arc::new(limiter_with_clock(clock));

        limiter.clone().start_cleanup();
        limiter.clone().start_cleanup();
        assert_eq!(limiter.cleanup_spawn_count(), 1);
    }

    #[test]
    fn record_success_only_clears_pending_success_and_keeps_active_block() {
        let clock = TestClock::new(0);
        let limiter = limiter_with_clock(clock.clone());

        // A successful request removes its just-recorded timestamp.
        assert_eq!(limiter.check("ip", 1, 100), None);
        limiter.record_success("ip");

        clock.set(10);
        assert_eq!(limiter.check("ip", 1, 100), None);

        // Failed requests keep their timestamp and can still trigger blocks.
        clock.set(20);
        assert_eq!(limiter.check("ip", 1, 100), Some(220));

        // Success does not clear an active block.
        limiter.record_success("ip");
        clock.set(21);
        assert_eq!(limiter.check("ip", 1, 100), Some(220));
    }
}
