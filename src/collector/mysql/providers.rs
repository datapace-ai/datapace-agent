//! MySQL/MariaDB cloud provider detection.
//!
//! Detects the cloud provider from connection URL patterns and database settings.

use std::collections::HashMap;

/// Detect the cloud provider from the connection URL
///
/// This is a quick check based on URL patterns. For more accurate detection,
/// use `detect_provider()` which also queries the database.
pub fn detect_provider_from_url(url: &str) -> String {
    let url_lower = url.to_lowercase();

    // AWS RDS / Aurora
    if url_lower.contains(".rds.amazonaws.com") {
        return "rds".to_string();
    }

    // Google Cloud SQL
    if url_lower.contains("cloudsql") || url_lower.contains(".google.com") {
        return "cloudsql".to_string();
    }

    // Azure Database for MySQL
    if url_lower.contains(".mysql.database.azure.com") {
        return "azure".to_string();
    }

    // PlanetScale
    if url_lower.contains("planetscale") || url_lower.contains(".psdb.cloud") {
        return "planetscale".to_string();
    }

    // DigitalOcean
    if url_lower.contains(".db.ondigitalocean.com") {
        return "digitalocean".to_string();
    }

    // Default to generic
    "generic".to_string()
}

// TODO: Implement when MySQL support is added
// /// Detect the cloud provider by querying database settings
// pub async fn detect_provider(
//     pool: &MySqlPool,
//     connection_url: &str,
// ) -> Result<String, CollectorError> {
//     // First check URL patterns
//     let from_url = detect_provider_from_url(connection_url);
//     if from_url != "generic" {
//         return Ok(from_url);
//     }
//
//     // Check for Aurora-specific variable
//     let result = sqlx::query_as::<_, (String, String)>(
//         "SHOW VARIABLES LIKE 'aurora_version'"
//     )
//     .fetch_optional(pool)
//     .await?;
//
//     if result.is_some() {
//         return Ok("aurora".to_string());
//     }
//
//     // Check for RDS-specific variable
//     let result = sqlx::query_as::<_, (String, String)>(
//         "SHOW VARIABLES LIKE 'rds_%'"
//     )
//     .fetch_optional(pool)
//     .await?;
//
//     if result.is_some() {
//         return Ok("rds".to_string());
//     }
//
//     Ok("generic".to_string())
// }

/// Get provider-specific metadata
///
/// Returns additional metadata based on the detected provider.
pub fn get_provider_metadata(_provider: &str) -> HashMap<String, String> {
    // TODO: Implement provider-specific metadata collection
    // For example:
    // - Aurora: aurora_version
    // - RDS: instance type, region
    // - PlanetScale: branch info
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_provider_from_url() {
        // AWS RDS
        assert_eq!(
            detect_provider_from_url("mysql://user:pass@mydb.abc123.us-east-1.rds.amazonaws.com:3306/db"),
            "rds"
        );

        // Google Cloud SQL
        assert_eq!(
            detect_provider_from_url("mysql://user:pass@/db?host=/cloudsql/project:region:instance"),
            "cloudsql"
        );

        // Azure
        assert_eq!(
            detect_provider_from_url("mysql://user:pass@myserver.mysql.database.azure.com/db"),
            "azure"
        );

        // PlanetScale
        assert_eq!(
            detect_provider_from_url("mysql://user:pass@aws.connect.psdb.cloud/db"),
            "planetscale"
        );

        // DigitalOcean
        assert_eq!(
            detect_provider_from_url("mysql://user:pass@db-mysql.db.ondigitalocean.com:25060/db"),
            "digitalocean"
        );

        // Generic
        assert_eq!(
            detect_provider_from_url("mysql://user:pass@localhost:3306/db"),
            "generic"
        );
    }
}
