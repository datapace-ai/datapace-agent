use regex::Regex;
use std::sync::LazyLock;

/// Sanitize a SQL query by scrubbing sensitive data patterns:
/// - Email addresses → `<email>`
/// - UUIDs → `<uuid>`
/// - IPv4/IPv6 addresses → `<ip>`
/// - JWT / Bearer tokens → `<token>`
/// - Credit card numbers → `<card>`
/// - String literals containing potential secrets → `'?'`
pub fn sanitize_query(sql: &str) -> String {
    let mut s = sql.to_string();

    // Order matters: scrub specific patterns before generic ones
    s = RE_EMAIL.replace_all(&s, "<email>").to_string();
    s = RE_UUID.replace_all(&s, "<uuid>").to_string();
    s = RE_IPV4.replace_all(&s, "<ip>").to_string();
    s = RE_JWT.replace_all(&s, "<token>").to_string();
    s = RE_BEARER.replace_all(&s, "Bearer <token>").to_string();
    s = RE_CREDIT_CARD.replace_all(&s, "<card>").to_string();

    // Replace remaining string literals that look like secrets
    s = RE_LONG_HEX_STRING.replace_all(&s, "'?'").to_string();
    s = RE_BASE64_STRING.replace_all(&s, "'?'").to_string();

    s
}

static RE_EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap());

static RE_UUID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
        .unwrap()
});

static RE_IPV4: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap());

static RE_JWT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"eyJ[a-zA-Z0-9_-]+\.eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+").unwrap());

static RE_BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Bearer\s+[a-zA-Z0-9._\-]+").unwrap());

static RE_CREDIT_CARD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}\b").unwrap());

// Hex strings >= 32 chars inside quotes (likely API keys, hashes)
static RE_LONG_HEX_STRING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"'[0-9a-fA-F]{32,}'").unwrap());

// Base64-like strings >= 20 chars inside quotes
static RE_BASE64_STRING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"'[A-Za-z0-9+/=]{20,}'").unwrap());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_email() {
        let s = sanitize_query("SELECT * FROM users WHERE email = 'alice@example.com'");
        assert!(!s.contains("alice@example.com"));
        assert!(s.contains("<email>"));
    }

    #[test]
    fn scrub_uuid() {
        let s = sanitize_query(
            "SELECT * FROM orders WHERE id = '550e8400-e29b-41d4-a716-446655440000'",
        );
        assert!(!s.contains("550e8400"));
        assert!(s.contains("<uuid>"));
    }

    #[test]
    fn scrub_ipv4() {
        let s = sanitize_query("SELECT * FROM logs WHERE ip = '192.168.1.100'");
        assert!(!s.contains("192.168.1.100"));
        assert!(s.contains("<ip>"));
    }

    #[test]
    fn scrub_jwt() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abc123_signature";
        let s = sanitize_query(&format!("SELECT * FROM tokens WHERE token = '{jwt}'"));
        assert!(!s.contains("eyJ"));
        assert!(s.contains("<token>"));
    }

    #[test]
    fn scrub_bearer() {
        let s = sanitize_query("SET header = 'Bearer sk_live_abc123def456'");
        assert!(!s.contains("sk_live"));
        assert!(s.contains("<token>"));
    }

    #[test]
    fn scrub_credit_card() {
        let s = sanitize_query("INSERT INTO payments (card) VALUES ('4111-1111-1111-1111')");
        assert!(!s.contains("4111"));
        assert!(s.contains("<card>"));
    }

    #[test]
    fn scrub_long_hex() {
        let hex = "a".repeat(64);
        let s = sanitize_query(&format!("SELECT * FROM keys WHERE key = '{hex}'"));
        assert!(!s.contains(&hex));
    }

    #[test]
    fn scrub_base64_token() {
        let b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefg=";
        let s = sanitize_query(&format!("SELECT * FROM sessions WHERE token = '{b64}'"));
        assert!(!s.contains(b64));
    }

    #[test]
    fn no_false_positive_short_string() {
        let s = sanitize_query("SELECT * FROM users WHERE name = 'Alice'");
        assert!(s.contains("Alice")); // Short non-sensitive string should remain
    }

    #[test]
    fn multiple_emails() {
        let s = sanitize_query("SELECT * FROM users WHERE email IN ('a@b.com', 'c@d.org')");
        assert!(!s.contains("a@b.com"));
        assert!(!s.contains("c@d.org"));
        let count = s.matches("<email>").count();
        assert_eq!(count, 2);
    }

    #[test]
    fn mixed_sensitive_data() {
        let s = sanitize_query(
            "INSERT INTO audit (email, ip, session) VALUES ('admin@corp.io', '10.0.0.1', 'eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig')",
        );
        assert!(!s.contains("admin@corp.io"));
        assert!(!s.contains("10.0.0.1"));
        assert!(!s.contains("eyJ"));
    }

    #[test]
    fn credit_card_no_dashes() {
        let s = sanitize_query("SELECT * FROM cards WHERE num = '4111111111111111'");
        assert!(!s.contains("4111111111111111"));
        assert!(s.contains("<card>"));
    }

    #[test]
    fn preserves_sql_structure() {
        let s = sanitize_query(
            "SELECT u.id, u.name FROM users u WHERE u.email = 'test@test.com' AND u.active = true",
        );
        assert!(s.contains("SELECT u.id"));
        assert!(s.contains("FROM users u"));
        assert!(s.contains("AND u.active = true"));
    }

    #[test]
    fn uuid_in_where_clause() {
        let s = sanitize_query(
            "DELETE FROM sessions WHERE user_id = 'a1b2c3d4-e5f6-7890-abcd-ef1234567890' AND expired = true",
        );
        assert!(s.contains("<uuid>"));
        assert!(s.contains("AND expired = true"));
    }

    #[test]
    fn empty_input() {
        let s = sanitize_query("");
        assert_eq!(s, "");
    }

    #[test]
    fn no_sensitive_data() {
        let sql = "SELECT count(*) FROM pg_stat_activity WHERE state = 'active'";
        let s = sanitize_query(sql);
        assert_eq!(s, sql); // Should be unchanged
    }

    #[test]
    fn ip_in_function_call() {
        let s = sanitize_query("SELECT inet '192.168.0.1' << inet '192.168.0.0/24'");
        assert!(!s.contains("192.168.0.1"));
        assert!(s.contains("<ip>"));
    }

    #[test]
    fn multiple_uuids() {
        let s = sanitize_query(
            "SELECT * FROM t WHERE a = '11111111-1111-1111-1111-111111111111' OR b = '22222222-2222-2222-2222-222222222222'",
        );
        let count = s.matches("<uuid>").count();
        assert_eq!(count, 2);
    }
}
