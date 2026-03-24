use super::activity::ActivityCollector;
use super::explain::ExplainCollector;
use super::io::IoCollector;
use super::locks::LocksCollector;
use super::mongodb::{
    MongoCollectionsCollector, MongoCurrentOpsCollector, MongoReplStatusCollector,
    MongoServerStatusCollector, MongoSlowQueriesCollector, MongoTopCollector,
};
use super::schema::SchemaCollector;
use super::statements::StatementsCollector;
use super::tables::TablesCollector;
use super::{Collector, CollectorInterval};

/// Metadata for a registered collector.
pub struct CollectorInfo {
    /// Unique name (must match `Collector::name()`).
    pub name: &'static str,
    /// Whether this collector runs on the fast or slow tick.
    pub interval: CollectorInterval,
    /// Factory function that creates an instance of the collector.
    pub factory: fn() -> Box<dyn Collector>,
}

/// All PostgreSQL collectors.
pub fn all_postgres_collectors() -> Vec<CollectorInfo> {
    vec![
        // ── Fast (volatile metrics, ~30s) ──
        CollectorInfo {
            name: "statements",
            interval: CollectorInterval::Fast,
            factory: || Box::new(StatementsCollector),
        },
        CollectorInfo {
            name: "activity",
            interval: CollectorInterval::Fast,
            factory: || Box::new(ActivityCollector),
        },
        CollectorInfo {
            name: "locks",
            interval: CollectorInterval::Fast,
            factory: || Box::new(LocksCollector),
        },
        CollectorInfo {
            name: "explain",
            interval: CollectorInterval::Fast,
            factory: || Box::new(ExplainCollector::default()),
        },
        // ── Slow (structural metrics, ~300s) ──
        CollectorInfo {
            name: "tables",
            interval: CollectorInterval::Slow,
            factory: || Box::new(TablesCollector),
        },
        CollectorInfo {
            name: "schema",
            interval: CollectorInterval::Slow,
            factory: || Box::new(SchemaCollector),
        },
        CollectorInfo {
            name: "io",
            interval: CollectorInterval::Slow,
            factory: || Box::new(IoCollector),
        },
    ]
}

/// All MongoDB collectors.
pub fn all_mongodb_collectors() -> Vec<CollectorInfo> {
    vec![
        // ── Fast ──
        CollectorInfo {
            name: "mongo_server_status",
            interval: CollectorInterval::Fast,
            factory: || Box::new(MongoServerStatusCollector),
        },
        CollectorInfo {
            name: "mongo_current_ops",
            interval: CollectorInterval::Fast,
            factory: || Box::new(MongoCurrentOpsCollector),
        },
        CollectorInfo {
            name: "mongo_slow_queries",
            interval: CollectorInterval::Fast,
            factory: || Box::new(MongoSlowQueriesCollector),
        },
        CollectorInfo {
            name: "mongo_top",
            interval: CollectorInterval::Fast,
            factory: || Box::new(MongoTopCollector),
        },
        // ── Slow ──
        CollectorInfo {
            name: "mongo_collections",
            interval: CollectorInterval::Slow,
            factory: || Box::new(MongoCollectionsCollector),
        },
        CollectorInfo {
            name: "mongo_repl_status",
            interval: CollectorInterval::Slow,
            factory: || Box::new(MongoReplStatusCollector),
        },
    ]
}

/// Backward-compatible alias — returns all PostgreSQL collectors.
pub fn all_collectors() -> Vec<CollectorInfo> {
    all_postgres_collectors()
}

/// All collectors for a given database type.
pub fn all_collectors_for(db_type: &str) -> Vec<CollectorInfo> {
    match db_type {
        "mongodb" => all_mongodb_collectors(),
        _ => all_postgres_collectors(),
    }
}

/// All registered collector names for the default (postgres) type.
pub fn all_collector_names() -> Vec<&'static str> {
    all_postgres_collectors().iter().map(|c| c.name).collect()
}

/// All collector names for a given database type.
pub fn all_collector_names_for(db_type: &str) -> Vec<&'static str> {
    all_collectors_for(db_type).iter().map(|c| c.name).collect()
}

/// Build collector instances for a given tick type, filtered by an allowed-names list.
///
/// - `db_type`: `"postgres"`, `"mongodb"`, etc.
/// - `tick_type`: `"fast"` or `"slow"`.
/// - `allowed_names`: only collectors whose name appears in this list are included.
pub fn build_collectors_for_tick(
    db_type: &str,
    tick_type: &str,
    allowed_names: &[String],
) -> Vec<Box<dyn Collector>> {
    let target_interval = match tick_type {
        "fast" => CollectorInterval::Fast,
        "slow" => CollectorInterval::Slow,
        _ => return vec![],
    };

    all_collectors_for(db_type)
        .into_iter()
        .filter(|info| {
            info.interval == target_interval && allowed_names.iter().any(|n| n == info.name)
        })
        .map(|info| (info.factory)())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_postgres_names_are_unique() {
        let names = all_collector_names();
        let unique: HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "Duplicate collector names found");
    }

    #[test]
    fn all_mongodb_names_are_unique() {
        let names = all_collector_names_for("mongodb");
        let unique: HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "Duplicate collector names found");
    }

    #[test]
    fn factory_names_match_info_postgres() {
        for info in all_postgres_collectors() {
            let instance = (info.factory)();
            assert_eq!(
                instance.name(),
                info.name,
                "CollectorInfo name doesn't match Collector::name()"
            );
            assert_eq!(
                instance.interval(),
                info.interval,
                "CollectorInfo interval doesn't match Collector::interval() for {}",
                info.name
            );
        }
    }

    #[test]
    fn factory_names_match_info_mongodb() {
        for info in all_mongodb_collectors() {
            let instance = (info.factory)();
            assert_eq!(
                instance.name(),
                info.name,
                "CollectorInfo name doesn't match Collector::name()"
            );
            assert_eq!(
                instance.interval(),
                info.interval,
                "CollectorInfo interval doesn't match Collector::interval() for {}",
                info.name
            );
        }
    }

    #[test]
    fn fast_tick_filters_correctly_postgres() {
        let allowed: Vec<String> = all_collector_names()
            .iter()
            .map(|n| n.to_string())
            .collect();
        let fast = build_collectors_for_tick("postgres", "fast", &allowed);
        let names: Vec<_> = fast.iter().map(|c| c.name()).collect();
        assert!(names.contains(&"statements"));
        assert!(names.contains(&"activity"));
        assert!(names.contains(&"locks"));
        assert!(names.contains(&"explain"));
        assert!(!names.contains(&"tables"));
        assert!(!names.contains(&"schema"));
        assert!(!names.contains(&"io"));
    }

    #[test]
    fn slow_tick_filters_correctly_postgres() {
        let allowed: Vec<String> = all_collector_names()
            .iter()
            .map(|n| n.to_string())
            .collect();
        let slow = build_collectors_for_tick("postgres", "slow", &allowed);
        let names: Vec<_> = slow.iter().map(|c| c.name()).collect();
        assert!(names.contains(&"tables"));
        assert!(names.contains(&"schema"));
        assert!(names.contains(&"io"));
        assert!(!names.contains(&"statements"));
    }

    #[test]
    fn fast_tick_filters_correctly_mongodb() {
        let allowed: Vec<String> = all_collector_names_for("mongodb")
            .iter()
            .map(|n| n.to_string())
            .collect();
        let fast = build_collectors_for_tick("mongodb", "fast", &allowed);
        let names: Vec<_> = fast.iter().map(|c| c.name()).collect();
        assert!(names.contains(&"mongo_server_status"));
        assert!(names.contains(&"mongo_current_ops"));
        assert!(names.contains(&"mongo_slow_queries"));
        assert!(names.contains(&"mongo_top"));
        assert!(!names.contains(&"mongo_collections"));
        assert!(!names.contains(&"mongo_repl_status"));
    }

    #[test]
    fn slow_tick_filters_correctly_mongodb() {
        let allowed: Vec<String> = all_collector_names_for("mongodb")
            .iter()
            .map(|n| n.to_string())
            .collect();
        let slow = build_collectors_for_tick("mongodb", "slow", &allowed);
        let names: Vec<_> = slow.iter().map(|c| c.name()).collect();
        assert!(names.contains(&"mongo_collections"));
        assert!(names.contains(&"mongo_repl_status"));
        assert!(!names.contains(&"mongo_server_status"));
    }

    #[test]
    fn allowed_list_filters_collectors() {
        let allowed = vec!["statements".to_string(), "tables".to_string()];
        let fast = build_collectors_for_tick("postgres", "fast", &allowed);
        let slow = build_collectors_for_tick("postgres", "slow", &allowed);
        assert_eq!(fast.len(), 1);
        assert_eq!(fast[0].name(), "statements");
        assert_eq!(slow.len(), 1);
        assert_eq!(slow[0].name(), "tables");
    }

    #[test]
    fn invalid_tick_type_returns_empty() {
        let allowed = vec!["statements".to_string()];
        let result = build_collectors_for_tick("postgres", "invalid", &allowed);
        assert!(result.is_empty());
    }

    #[test]
    fn all_collectors_for_returns_correct_type() {
        let pg = all_collectors_for("postgres");
        assert_eq!(pg.len(), 7);
        let mongo = all_collectors_for("mongodb");
        assert_eq!(mongo.len(), 6);
    }
}
