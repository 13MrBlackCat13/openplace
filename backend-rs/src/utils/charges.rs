use chrono::{DateTime, Utc};

/// Port of src/utils/charges.ts: fractional lazy charge regeneration.
/// `current` is the value as of `last_updated`; regen is computed on read.
pub fn regenerate(
    current: f64,
    max: f64,
    cooldown_ms: i32,
    last_updated: DateTime<Utc>,
    now: DateTime<Utc>,
) -> f64 {
    if current >= max {
        return current;
    }
    let generated =
        (now - last_updated).num_milliseconds() as f64 / cooldown_ms.clamp(1, i32::MAX) as f64;
    (current + generated).min(max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn no_regen_when_full() {
        let now = Utc::now();
        assert_eq!(regenerate(20.0, 20.0, 30_000, now, now), 20.0);
    }

    #[test]
    fn fractional_regen() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t1 = t0 + chrono::Duration::milliseconds(15_000);
        let charged = regenerate(10.0, 20.0, 30_000, t0, t1);
        assert!((charged - 10.5).abs() < 1e-9);
    }

    #[test]
    fn regen_caps_at_max() {
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t1 = t0 + chrono::Duration::hours(24);
        assert_eq!(regenerate(10.0, 20.0, 30_000, t0, t1), 20.0);
    }
}
