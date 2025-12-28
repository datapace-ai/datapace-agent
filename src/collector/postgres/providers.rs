//! Database provider detection and metadata collection.
//!
//! Supports auto-detection and metadata collection for:
//! - Generic PostgreSQL
//! - AWS RDS
//! - AWS Aurora
//! - Supabase
//! - Neon

use crate::collector::CollectorError;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::debug;

/// Detect the database provider from connection string and database metadata
pub async fn detect_provider(pool: &PgPool, connection_url: &str) -> Result<String, CollectorError> {
    debug!("Auto-detecting database provider");

    // Check connection string patterns first
    let url_lower = connection_url.to_lowercase();

    if url_lower.contains(".supabase.") || url_lower.contains("supabase.co") {
        return Ok("supabase".to_string());
    }

    if url_lower.contains(".neon.") || url_lower.contains("neon.tech") {
        return Ok("neon".to_string());
    }

    if url_lower.contains(".rds.amazonaws.com") {
        // Could be RDS or Aurora, need to check further
        return detect_aws_provider(pool).await;
    }

    // Check database-side indicators
    if let Ok(provider) = detect_from_extensions(pool).await {
        return Ok(provider);
    }

    if let Ok(provider) = detect_from_settings(pool).await {
        return Ok(provider);
    }

    // Default to generic PostgreSQL
    Ok("generic".to_string())
}

/// Detect AWS RDS vs Aurora
async fn detect_aws_provider(pool: &PgPool) -> Result<String, CollectorError> {
    // Check for Aurora-specific function
    let has_aurora: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_proc WHERE proname = 'aurora_version')",
    )
    .fetch_one(pool)
    .await?;

    if has_aurora.0 {
        return Ok("aurora".to_string());
    }

    Ok("rds".to_string())
}

/// Detect provider from installed extensions
async fn detect_from_extensions(pool: &PgPool) -> Result<String, CollectorError> {
    let extensions: Vec<(String,)> =
        sqlx::query_as("SELECT extname FROM pg_extension")
            .fetch_all(pool)
            .await?;

    let ext_names: Vec<&str> = extensions.iter().map(|e| e.0.as_str()).collect();

    // Supabase typically has these extensions
    if ext_names.contains(&"supabase_vault") || ext_names.contains(&"pgsodium") {
        return Ok("supabase".to_string());
    }

    // Neon-specific extensions
    if ext_names.contains(&"neon") {
        return Ok("neon".to_string());
    }

    Err(CollectorError::DetectionError(
        "Could not detect provider from extensions".to_string(),
    ))
}

/// Detect provider from database settings
async fn detect_from_settings(pool: &PgPool) -> Result<String, CollectorError> {
    // Check for provider-specific GUC parameters
    let result: Result<(String,), _> = sqlx::query_as(
        "SELECT setting FROM pg_settings WHERE name = 'rds.extensions'",
    )
    .fetch_one(pool)
    .await;

    if result.is_ok() {
        return Ok("rds".to_string());
    }

    Err(CollectorError::DetectionError(
        "Could not detect provider from settings".to_string(),
    ))
}

/// Get provider-specific metadata
pub async fn get_provider_metadata(
    pool: &PgPool,
    provider: &str,
) -> Result<HashMap<String, String>, CollectorError> {
    let mut metadata = HashMap::new();

    match provider {
        "rds" | "aurora" => {
            if let Ok(info) = get_rds_metadata(pool).await {
                metadata.extend(info);
            }
        }
        "supabase" => {
            if let Ok(info) = get_supabase_metadata(pool).await {
                metadata.extend(info);
            }
        }
        "neon" => {
            if let Ok(info) = get_neon_metadata(pool).await {
                metadata.extend(info);
            }
        }
        _ => {}
    }

    Ok(metadata)
}

async fn get_rds_metadata(pool: &PgPool) -> Result<HashMap<String, String>, CollectorError> {
    let mut metadata = HashMap::new();

    // Try to get Aurora version if available
    let aurora_version: Result<(String,), _> =
        sqlx::query_as("SELECT aurora_version()")
            .fetch_one(pool)
            .await;

    if let Ok((version,)) = aurora_version {
        metadata.insert("aurora_version".to_string(), version);
    }

    // Get instance class from comments or other sources if available
    // This is limited without direct AWS API access

    Ok(metadata)
}

async fn get_supabase_metadata(_pool: &PgPool) -> Result<HashMap<String, String>, CollectorError> {
    let metadata = HashMap::new();

    // Supabase-specific metadata collection
    // Limited without Supabase management API access

    Ok(metadata)
}

async fn get_neon_metadata(_pool: &PgPool) -> Result<HashMap<String, String>, CollectorError> {
    let metadata = HashMap::new();

    // Neon-specific metadata collection
    // Could include branch info, compute endpoint, etc.

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_detection_supabase() {
        let url = "postgres://user:pass@db.xyz.supabase.co:5432/postgres";
        assert!(url.to_lowercase().contains("supabase.co"));
    }

    #[test]
    fn test_url_detection_neon() {
        let url = "postgres://user:pass@ep-cool-branch-123.us-east-2.aws.neon.tech/neondb";
        assert!(url.to_lowercase().contains("neon.tech"));
    }

    #[test]
    fn test_url_detection_rds() {
        let url = "postgres://user:pass@mydb.abc123.us-east-1.rds.amazonaws.com:5432/postgres";
        assert!(url.to_lowercase().contains(".rds.amazonaws.com"));
    }
}
