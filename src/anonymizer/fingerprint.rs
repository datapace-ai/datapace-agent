use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

/// Normalize a SQL query and return a 16-char hex fingerprint.
///
/// Steps:
/// 1. Collapse whitespace
/// 2. Lowercase
/// 3. Replace literal values with `?`
/// 4. SHA-256 → first 8 bytes → 16 hex chars
pub fn fingerprint_query(sql: &str) -> String {
    let normalized = normalize(sql);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    let hash = hasher.finalize();
    hex::encode(&hash[..8])
}

/// Return the normalized form of a query (useful for display alongside the fingerprint).
pub fn normalize(sql: &str) -> String {
    let mut s = sql.to_string();

    // Remove comments
    s = RE_LINE_COMMENT.replace_all(&s, " ").to_string();
    s = RE_BLOCK_COMMENT.replace_all(&s, " ").to_string();

    // Collapse whitespace
    s = RE_WHITESPACE.replace_all(&s, " ").to_string();
    let s = s.trim().to_lowercase();

    // Replace string literals
    let s = RE_STRING_LITERAL.replace_all(&s, "?").to_string();

    // Replace numeric literals (integers and floats)
    let s = RE_NUMERIC_LITERAL.replace_all(&s, "?").to_string();

    // Replace $N parameters
    let s = RE_PARAM.replace_all(&s, "?").to_string();

    // Collapse IN lists: in (?, ?, ?) → in (?)
    let s = RE_IN_LIST.replace_all(&s, "in (?)").to_string();

    s
}

static RE_LINE_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--[^\n]*").unwrap());
static RE_BLOCK_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/\*[\s\S]*?\*/").unwrap());
static RE_WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static RE_STRING_LITERAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"'[^']*'").unwrap());
static RE_NUMERIC_LITERAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d+(\.\d+)?\b").unwrap());
static RE_PARAM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\d+").unwrap());
static RE_IN_LIST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)in\s*\(\s*\?(?:\s*,\s*\?)*\s*\)").unwrap());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_select() {
        let fp = fingerprint_query("SELECT * FROM users WHERE id = 42");
        assert_eq!(fp.len(), 16);
    }

    #[test]
    fn same_structure_same_fingerprint() {
        let a = fingerprint_query("SELECT * FROM users WHERE id = 1");
        let b = fingerprint_query("SELECT * FROM users WHERE id = 999");
        assert_eq!(a, b);
    }

    #[test]
    fn different_tables_different_fingerprint() {
        let a = fingerprint_query("SELECT * FROM users WHERE id = 1");
        let b = fingerprint_query("SELECT * FROM orders WHERE id = 1");
        assert_ne!(a, b);
    }

    #[test]
    fn normalizes_whitespace() {
        let a = fingerprint_query("SELECT   *  FROM   users");
        let b = fingerprint_query("SELECT * FROM users");
        assert_eq!(a, b);
    }

    #[test]
    fn normalizes_case() {
        let a = fingerprint_query("SELECT * FROM users");
        let b = fingerprint_query("select * from USERS");
        assert_eq!(a, b);
    }

    #[test]
    fn replaces_string_literals() {
        let n = normalize("SELECT * FROM users WHERE name = 'Alice'");
        assert!(n.contains("?"));
        assert!(!n.contains("Alice"));
    }

    #[test]
    fn replaces_numeric_literals() {
        let n = normalize("SELECT * FROM users WHERE age > 25 AND score = 99.5");
        assert!(!n.contains("25"));
        assert!(!n.contains("99.5"));
    }

    #[test]
    fn replaces_params() {
        let n = normalize("SELECT * FROM users WHERE id = $1 AND name = $2");
        assert!(!n.contains("$1"));
        assert!(!n.contains("$2"));
    }

    #[test]
    fn collapses_in_list() {
        let n = normalize("SELECT * FROM users WHERE id IN (1, 2, 3, 4, 5)");
        assert!(n.contains("in (?)"), "got: {n}");
    }

    #[test]
    fn removes_line_comments() {
        let n = normalize("SELECT * FROM users -- get all users\nWHERE id = 1");
        assert!(!n.contains("get all"));
    }

    #[test]
    fn removes_block_comments() {
        let n = normalize("SELECT /* important */ * FROM users WHERE id = 1");
        assert!(!n.contains("important"));
    }

    #[test]
    fn insert_normalization() {
        let a = fingerprint_query("INSERT INTO users (name) VALUES ('Alice')");
        let b = fingerprint_query("INSERT INTO users (name) VALUES ('Bob')");
        assert_eq!(a, b);
    }

    #[test]
    fn update_normalization() {
        let a = fingerprint_query("UPDATE users SET name = 'Alice' WHERE id = 1");
        let b = fingerprint_query("UPDATE users SET name = 'Bob' WHERE id = 2");
        assert_eq!(a, b);
    }

    #[test]
    fn delete_normalization() {
        let a = fingerprint_query("DELETE FROM users WHERE id = 100");
        let b = fingerprint_query("DELETE FROM users WHERE id = 200");
        assert_eq!(a, b);
    }

    #[test]
    fn complex_join() {
        let sql = "SELECT u.name, o.total FROM users u JOIN orders o ON u.id = o.user_id WHERE o.total > 100.50 AND u.created_at > '2024-01-01'";
        let n = normalize(sql);
        assert!(!n.contains("100.50"));
        assert!(!n.contains("2024-01-01"));
    }

    #[test]
    fn empty_string() {
        let fp = fingerprint_query("");
        assert_eq!(fp.len(), 16);
    }

    #[test]
    fn subquery() {
        let a = fingerprint_query(
            "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders WHERE total > 100)",
        );
        let b = fingerprint_query(
            "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders WHERE total > 500)",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn cte_query() {
        let a = fingerprint_query(
            "WITH active AS (SELECT * FROM users WHERE active = true) SELECT * FROM active WHERE id = 1",
        );
        let b = fingerprint_query(
            "WITH active AS (SELECT * FROM users WHERE active = true) SELECT * FROM active WHERE id = 99",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn limit_offset() {
        let a = fingerprint_query("SELECT * FROM users LIMIT 10 OFFSET 20");
        let b = fingerprint_query("SELECT * FROM users LIMIT 50 OFFSET 100");
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_is_hex() {
        let fp = fingerprint_query("SELECT 1");
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
