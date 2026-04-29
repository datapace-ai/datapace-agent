//! MongoDB provider detection.
//!
//! Returns one of:
//! - `"atlas"` — MongoDB Atlas (managed)
//! - `"documentdb"` — AWS DocumentDB (Mongo-compatible API)
//! - `"cosmosdb"` — Azure Cosmos DB MongoDB API
//! - `"generic"` — self-managed or unknown
//!
//! URL pattern matching is the primary fast-path. A live `hello`/`buildInfo`
//! probe is used only when the URL is ambiguous and a [`mongodb::Client`] is
//! available.

use mongodb::bson::doc;
use mongodb::Client;
use std::collections::HashMap;

pub const PROVIDER_ATLAS: &str = "atlas";
pub const PROVIDER_DOCUMENTDB: &str = "documentdb";
pub const PROVIDER_COSMOSDB: &str = "cosmosdb";
pub const PROVIDER_GENERIC: &str = "generic";

/// Detect provider from a connection URL alone.
///
/// Returns `Some(provider)` if the URL definitively identifies a managed
/// service, `None` if a runtime probe is needed.
pub fn detect_from_url(url: &str) -> Option<&'static str> {
    let lower = url.to_lowercase();
    if lower.contains(".mongodb.net") {
        return Some(PROVIDER_ATLAS);
    }
    if lower.contains(".docdb.amazonaws.com") {
        return Some(PROVIDER_DOCUMENTDB);
    }
    if lower.contains(".cosmos.azure.com") || lower.contains(".documents.azure.com") {
        return Some(PROVIDER_COSMOSDB);
    }
    None
}

/// Detect provider, falling back to a live probe via `buildInfo`/`hello` when
/// the URL is ambiguous.
pub async fn detect_provider(client: &Client, url: &str) -> &'static str {
    if let Some(p) = detect_from_url(url) {
        return p;
    }
    // Probe `buildInfo` for distinctive fields.
    if let Ok(info) = client
        .database("admin")
        .run_command(doc! { "buildInfo": 1 })
        .await
    {
        // Cosmos DB exposes a `cosmosdb` marker on some builds.
        if let Ok(modules) = info.get_array("modules") {
            if modules
                .iter()
                .any(|v| v.as_str().map(|s| s.contains("cosmos")).unwrap_or(false))
            {
                return PROVIDER_COSMOSDB;
            }
        }
        // DocumentDB historically reports an `engineVersion` field that mongo doesn't.
        if info.get_str("engineVersion").is_ok() {
            return PROVIDER_DOCUMENTDB;
        }
    }
    PROVIDER_GENERIC
}

/// Extract any cheap, URL-derivable metadata for the detected provider.
///
/// Region/instance-class lookups would require provider-specific control-plane
/// APIs (Atlas API key, AWS describe-cluster, etc.) and are deferred.
pub fn provider_metadata(provider: &str, url: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if provider == PROVIDER_ATLAS {
        if let Some(host) = extract_host(url) {
            out.insert("atlas_host".to_string(), host.clone());
            // SRV records like `cluster0.ab1cd.mongodb.net` — first segment is
            // the cluster name, second is the project hash.
            let parts: Vec<&str> = host.split('.').collect();
            if parts.len() >= 3 {
                out.insert("atlas_cluster".to_string(), parts[0].to_string());
                out.insert("atlas_project_hash".to_string(), parts[1].to_string());
            }
        }
    }
    out
}

/// Crude host extraction — pulls the first authority component out of a URL
/// without bringing in the `url` crate.
fn extract_host(url: &str) -> Option<String> {
    let after_scheme = url.split("://").nth(1)?;
    let after_auth = after_scheme.split('@').next_back()?;
    let host_only = after_auth.split('/').next()?.split('?').next()?;
    Some(host_only.split(':').next()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_detects_atlas() {
        assert_eq!(
            detect_from_url("mongodb+srv://user:p@cluster0.ab1cd.mongodb.net/db"),
            Some(PROVIDER_ATLAS)
        );
    }

    #[test]
    fn url_detects_documentdb() {
        assert_eq!(
            detect_from_url("mongodb://user:p@my-cluster.cluster-abc.us-east-1.docdb.amazonaws.com:27017/"),
            Some(PROVIDER_DOCUMENTDB)
        );
    }

    #[test]
    fn url_detects_cosmosdb() {
        assert_eq!(
            detect_from_url("mongodb://user@account.mongo.cosmos.azure.com:10255/"),
            Some(PROVIDER_COSMOSDB)
        );
        assert_eq!(
            detect_from_url("mongodb://user@account.documents.azure.com:10255/"),
            Some(PROVIDER_COSMOSDB)
        );
    }

    #[test]
    fn url_returns_none_for_self_managed() {
        assert_eq!(detect_from_url("mongodb://root:root@localhost:27017"), None);
    }

    #[test]
    fn atlas_metadata_extracts_cluster_and_project() {
        let meta = provider_metadata(
            PROVIDER_ATLAS,
            "mongodb+srv://u:p@cluster0.ab1cd.mongodb.net/db",
        );
        assert_eq!(meta.get("atlas_cluster").unwrap(), "cluster0");
        assert_eq!(meta.get("atlas_project_hash").unwrap(), "ab1cd");
    }

    #[test]
    fn extract_host_handles_credentials() {
        assert_eq!(
            extract_host("mongodb://user:pw@host.example.com:27017/db?retryWrites=true"),
            Some("host.example.com".to_string())
        );
    }
}
