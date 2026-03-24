use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

/// Compute a stable idempotency key for a snapshot.
///
/// Hash `source_id + collector + collected_at.timestamp()` with SHA-256,
/// returning the first 8 bytes as 16 hex chars (same pattern as `fingerprint_query`).
pub fn idempotency_key(source: &str, collector: &str, collected_at: &DateTime<Utc>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    hasher.update(b":");
    hasher.update(collector.as_bytes());
    hasher.update(b":");
    hasher.update(
        collected_at
            .timestamp_nanos_opt()
            .unwrap_or(0)
            .to_le_bytes(),
    );
    let hash = hasher.finalize();
    hex::encode(&hash[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn deterministic() {
        let ts = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let a = idempotency_key("db1", "statements", &ts);
        let b = idempotency_key("db1", "statements", &ts);
        assert_eq!(a, b);
    }

    #[test]
    fn length_and_hex() {
        let ts = Utc::now();
        let key = idempotency_key("db1", "statements", &ts);
        assert_eq!(key.len(), 16);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn different_source_different_key() {
        let ts = Utc::now();
        let a = idempotency_key("db1", "statements", &ts);
        let b = idempotency_key("db2", "statements", &ts);
        assert_ne!(a, b);
    }

    #[test]
    fn different_collector_different_key() {
        let ts = Utc::now();
        let a = idempotency_key("db1", "statements", &ts);
        let b = idempotency_key("db1", "activity", &ts);
        assert_ne!(a, b);
    }

    #[test]
    fn different_time_different_key() {
        let t1 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 1).unwrap();
        let a = idempotency_key("db1", "statements", &t1);
        let b = idempotency_key("db1", "statements", &t2);
        assert_ne!(a, b);
    }
}
