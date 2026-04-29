//! MongoDB schema inference via document sampling.
//!
//! `SchemaWalker` accumulates per-path statistics across a sample of BSON
//! documents and emits one [`ColumnMetadata`] per unique path. Paths are
//! flattened with dot notation for nested objects (`address.street`) and
//! `[]` segments for array elements (`photos[]`, `photos[].url`).
//!
//! All observation work runs in `O(paths × document_size)` time. Memory is
//! bounded by the per-path distinct-value cap and the global byte ceiling
//! (see module-level constants) so even pathologically polymorphic
//! collections cannot exhaust process memory.

use crate::collector::mongodb::bson_type::bson_type_label;
use crate::payload::ColumnMetadata;
use indexmap::IndexMap;
use mongodb::bson::{Bson, Document};
use std::collections::{BTreeSet, HashSet};

/// Default minimum sample size — collections smaller than this are sampled in full.
pub const MIN_SAMPLES: i64 = 100;
/// Default maximum sample size, regardless of collection cardinality.
pub const MAX_SAMPLES: i64 = 1_000;
/// Target sample ratio when collection is between MIN and MAX bounds.
pub const TARGET_RATIO: f64 = 0.01;

/// Maximum recursion depth before paths are truncated.
pub const MAX_DEPTH: u8 = 16;
/// Maximum unique paths tracked per collection.
pub const MAX_PATHS_PER_COLLECTION: usize = 5_000;
/// Maximum array elements walked per document at any single `[]` level.
pub const MAX_ARRAY_ELEMENTS_PER_DOC: usize = 100;

/// Per-path distinct value cap. Once exceeded, `distinct_capped = true`.
pub const DISTINCT_VALUE_CAP: usize = 1_000;
/// Reservoir size for sample values per path.
pub const SAMPLE_VALUES_PER_PATH: usize = 5;
/// Global kill-switch for distinct-tracking memory. When exceeded, all paths
/// switch to capped mode (no more distinct accumulation).
pub const MAX_DISTINCT_TRACKED_BYTES: usize = 256 * 1024 * 1024;

/// Pick an appropriate sample size for a given collection cardinality.
///
/// - Tiny collections (`<= MIN_SAMPLES`): sample everything.
/// - Mid-sized: sample 1 % (clamped to MIN..=MAX).
/// - Huge: capped at MAX_SAMPLES.
pub fn pick_sample_size(doc_count: i64) -> i64 {
    if doc_count <= 0 {
        return MIN_SAMPLES;
    }
    let target = (doc_count as f64 * TARGET_RATIO).ceil() as i64;
    target.clamp(MIN_SAMPLES.min(doc_count), MAX_SAMPLES)
}

/// Accumulator for schema inference across a stream of BSON documents.
pub struct SchemaWalker {
    paths: IndexMap<String, PathStats>,
    docs_sampled: i64,
    truncated_paths: u64,
    distinct_bytes: usize,
    distinct_globally_capped: bool,
    next_position: i32,
}

struct PathStats {
    seen_count: i64,
    null_count: i64,
    types: BTreeSet<&'static str>,
    distinct: HashSet<String>, // serialized BSON repr → cheap, deterministic hashable
    distinct_capped: bool,
    samples: Vec<serde_json::Value>, // reservoir, len <= SAMPLE_VALUES_PER_PATH
    samples_seen: u64,
    array_max_len: Option<i64>,
    is_array_element: bool,
    first_seen_position: i32,
}

impl SchemaWalker {
    pub fn new() -> Self {
        Self {
            paths: IndexMap::new(),
            docs_sampled: 0,
            truncated_paths: 0,
            distinct_bytes: 0,
            distinct_globally_capped: false,
            next_position: 1,
        }
    }

    /// Number of documents observed so far (denominator for presence_rate).
    pub fn docs_sampled(&self) -> i64 {
        self.docs_sampled
    }

    /// Truncated-path counter, exposed for tracing/diagnostics.
    pub fn truncated_paths(&self) -> u64 {
        self.truncated_paths
    }

    /// Observe one top-level document.
    pub fn observe_document(&mut self, doc: &Document) {
        self.docs_sampled += 1;
        let mut visited_in_doc: HashSet<String> = HashSet::new();
        for (key, value) in doc.iter() {
            self.observe_path(key, value, 1, false, &mut visited_in_doc);
        }
    }

    fn observe_path(
        &mut self,
        path: &str,
        value: &Bson,
        depth: u8,
        in_array: bool,
        visited_in_doc: &mut HashSet<String>,
    ) {
        if depth > MAX_DEPTH {
            self.record_path(path, "<truncated>", value, in_array, visited_in_doc);
            self.truncated_paths += 1;
            return;
        }

        // Record the value at this path (counts toward presence even for containers).
        self.record_path(path, bson_type_label(value), value, in_array, visited_in_doc);

        // Recurse into structural children.
        match value {
            Bson::Document(inner) => {
                for (k, v) in inner.iter() {
                    let child_path = format!("{}.{}", path, k);
                    self.observe_path(&child_path, v, depth + 1, in_array, visited_in_doc);
                }
            }
            Bson::Array(items) => {
                let array_path = format!("{}[]", path);
                // Track max length even if elements are clipped.
                if let Some(stats) = self.paths.get_mut(path) {
                    let len = items.len() as i64;
                    stats.array_max_len = Some(stats.array_max_len.map_or(len, |m| m.max(len)));
                }
                for item in items.iter().take(MAX_ARRAY_ELEMENTS_PER_DOC) {
                    self.observe_path(&array_path, item, depth + 1, true, visited_in_doc);
                }
            }
            _ => {}
        }
    }

    fn record_path(
        &mut self,
        path: &str,
        type_label: &'static str,
        value: &Bson,
        in_array: bool,
        visited_in_doc: &mut HashSet<String>,
    ) {
        let already_seen_this_doc = visited_in_doc.contains(path);

        // Refuse new paths once the cap is hit.
        if !self.paths.contains_key(path) && self.paths.len() >= MAX_PATHS_PER_COLLECTION {
            self.truncated_paths += 1;
            return;
        }

        let position = self.next_position;
        let stats = self.paths.entry(path.to_string()).or_insert_with(|| PathStats {
            seen_count: 0,
            null_count: 0,
            types: BTreeSet::new(),
            distinct: HashSet::new(),
            distinct_capped: false,
            samples: Vec::new(),
            samples_seen: 0,
            array_max_len: None,
            is_array_element: in_array,
            first_seen_position: position,
        });
        if stats.first_seen_position == position {
            self.next_position += 1;
        }

        if !already_seen_this_doc {
            stats.seen_count += 1;
            visited_in_doc.insert(path.to_string());
        }
        if matches!(value, Bson::Null) {
            stats.null_count += 1;
        }
        stats.types.insert(type_label);
        if in_array {
            stats.is_array_element = true;
        }

        // Distinct tracking — leaves only.
        let is_leaf = !matches!(value, Bson::Document(_) | Bson::Array(_));
        if is_leaf && !stats.distinct_capped && !self.distinct_globally_capped {
            let key = serde_json::to_string(&bson_to_json(value)).unwrap_or_default();
            if stats.distinct.len() < DISTINCT_VALUE_CAP {
                let added = stats.distinct.insert(key.clone());
                if added {
                    self.distinct_bytes += key.len();
                    if self.distinct_bytes > MAX_DISTINCT_TRACKED_BYTES {
                        self.distinct_globally_capped = true;
                    }
                }
            } else {
                stats.distinct_capped = true;
            }
        }

        // Reservoir sampling (Algorithm R) — deterministic surrogate using
        // a Knuth multiplicative hash of samples_seen as the index source.
        if is_leaf {
            stats.samples_seen += 1;
            let json = bson_to_json(value);
            if stats.samples.len() < SAMPLE_VALUES_PER_PATH {
                stats.samples.push(json);
            } else {
                let n = stats.samples_seen;
                let h = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32;
                let j = (h as usize) % (n as usize);
                if j < SAMPLE_VALUES_PER_PATH {
                    stats.samples[j] = json;
                }
            }
        }
    }

    /// Emit one [`ColumnMetadata`] per observed path, ordered by first-seen position.
    pub fn into_columns(self) -> Vec<ColumnMetadata> {
        let denom = self.docs_sampled.max(1) as f64;
        let globally_capped = self.distinct_globally_capped;
        let mut entries: Vec<(String, PathStats)> = self.paths.into_iter().collect();
        entries.sort_by_key(|(_, s)| s.first_seen_position);

        entries
            .into_iter()
            .map(|(path, s)| {
                let presence_rate = s.seen_count as f64 / denom;
                let null_rate = s.null_count as f64 / denom;
                let nullable = s.null_count > 0 || presence_rate < 1.0;

                let data_type = if s.types.len() == 1 {
                    s.types.iter().next().copied().unwrap_or("unknown").to_string()
                } else if s.types.is_empty() {
                    "unknown".to_string()
                } else {
                    "mixed".to_string()
                };

                let bson_types: Vec<String> = s.types.iter().map(|t| (*t).to_string()).collect();
                let distinct_capped = s.distinct_capped || globally_capped;
                let distinct_count = if globally_capped && s.distinct.is_empty() {
                    None
                } else {
                    Some(s.distinct.len() as i64)
                };

                ColumnMetadata {
                    name: path,
                    data_type,
                    nullable,
                    default: None,
                    position: s.first_seen_position,
                    presence_rate: Some(presence_rate),
                    null_rate: Some(null_rate),
                    bson_types: if bson_types.is_empty() { None } else { Some(bson_types) },
                    distinct_count,
                    distinct_capped: Some(distinct_capped),
                    sample_values: if s.samples.is_empty() { None } else { Some(s.samples) },
                    is_array_element: Some(s.is_array_element),
                    array_max_len: s.array_max_len,
                }
            })
            .collect()
    }
}

impl Default for SchemaWalker {
    fn default() -> Self {
        Self::new()
    }
}

/// Coerce a BSON value into a JSON value suitable for transport.
///
/// Lossy-but-stable conversions: ObjectId → hex string, DateTime → ISO 8601,
/// Decimal128 → string, Binary → base64-style debug. The MongoDB driver's
/// extended-JSON serializer would also work but produces noisier output;
/// for sample values we want compact, human-readable representations.
fn bson_to_json(value: &Bson) -> serde_json::Value {
    match value {
        Bson::Double(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Bson::String(s) => serde_json::Value::String(s.clone()),
        Bson::Boolean(b) => serde_json::Value::Bool(*b),
        Bson::Null | Bson::Undefined => serde_json::Value::Null,
        Bson::Int32(i) => serde_json::Value::Number((*i).into()),
        Bson::Int64(i) => serde_json::Value::Number((*i).into()),
        Bson::ObjectId(oid) => serde_json::Value::String(oid.to_hex()),
        Bson::DateTime(dt) => serde_json::Value::String(dt.try_to_rfc3339_string().unwrap_or_default()),
        Bson::Decimal128(d) => serde_json::Value::String(d.to_string()),
        Bson::Document(_) | Bson::Array(_) => {
            // For samples we don't recurse — surfaces as a placeholder.
            serde_json::Value::String(format!("{:?}", value))
        }
        other => serde_json::Value::String(format!("{:?}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::doc;

    fn names(cols: &[ColumnMetadata]) -> Vec<&str> {
        cols.iter().map(|c| c.name.as_str()).collect()
    }

    fn col<'a>(cols: &'a [ColumnMetadata], name: &str) -> &'a ColumnMetadata {
        cols.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("path {name} missing"))
    }

    #[test]
    fn flat_document() {
        let mut w = SchemaWalker::new();
        w.observe_document(&doc! { "a": 1, "b": "x" });
        let cols = w.into_columns();
        assert_eq!(names(&cols), vec!["a", "b"]);
        assert_eq!(col(&cols, "a").presence_rate, Some(1.0));
        assert_eq!(col(&cols, "a").data_type, "int32");
        assert_eq!(col(&cols, "b").data_type, "string");
    }

    #[test]
    fn nested_object_flattens_with_dots() {
        let mut w = SchemaWalker::new();
        w.observe_document(&doc! { "address": { "street": "x", "zip": 5_i32 } });
        let cols = w.into_columns();
        let n = names(&cols);
        assert!(n.contains(&"address"));
        assert!(n.contains(&"address.street"));
        assert!(n.contains(&"address.zip"));
        assert_eq!(col(&cols, "address.street").data_type, "string");
    }

    #[test]
    fn array_of_scalars() {
        let mut w = SchemaWalker::new();
        w.observe_document(&doc! { "tags": ["a", "b", "c"] });
        let cols = w.into_columns();
        let arr = col(&cols, "tags");
        assert_eq!(arr.array_max_len, Some(3));
        assert_eq!(col(&cols, "tags[]").data_type, "string");
        assert_eq!(col(&cols, "tags[]").is_array_element, Some(true));
    }

    #[test]
    fn array_of_documents() {
        let mut w = SchemaWalker::new();
        w.observe_document(&doc! { "photos": [{ "url": "u1" }, { "url": "u2" }] });
        let cols = w.into_columns();
        let n = names(&cols);
        assert!(n.contains(&"photos"));
        assert!(n.contains(&"photos[]"));
        assert!(n.contains(&"photos[].url"));
        assert_eq!(col(&cols, "photos[].url").is_array_element, Some(true));
    }

    #[test]
    fn polymorphic_field_marked_mixed() {
        let mut w = SchemaWalker::new();
        w.observe_document(&doc! { "v": 1_i32 });
        w.observe_document(&doc! { "v": "x" });
        w.observe_document(&doc! { "v": Bson::Null });
        let cols = w.into_columns();
        let v = col(&cols, "v");
        assert_eq!(v.data_type, "mixed");
        let types = v.bson_types.as_ref().unwrap();
        assert!(types.contains(&"int32".to_string()));
        assert!(types.contains(&"string".to_string()));
        assert!(types.contains(&"null".to_string()));
        assert!((v.null_rate.unwrap() - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn presence_rate_for_sparse_field() {
        let mut w = SchemaWalker::new();
        for _ in 0..7 {
            w.observe_document(&doc! { "a": 1 });
        }
        for _ in 0..3 {
            w.observe_document(&doc! { "a": 1, "b": "y" });
        }
        let cols = w.into_columns();
        assert!((col(&cols, "b").presence_rate.unwrap() - 0.3).abs() < 1e-9);
        assert_eq!(col(&cols, "a").presence_rate, Some(1.0));
    }

    #[test]
    fn distinct_cap_flips_when_exceeded() {
        let mut w = SchemaWalker::new();
        for i in 0..(DISTINCT_VALUE_CAP + 500) {
            w.observe_document(&doc! { "x": i as i64 });
        }
        let cols = w.into_columns();
        let x = col(&cols, "x");
        assert_eq!(x.distinct_count, Some(DISTINCT_VALUE_CAP as i64));
        assert_eq!(x.distinct_capped, Some(true));
    }

    #[test]
    fn reservoir_holds_at_most_n_samples() {
        let mut w = SchemaWalker::new();
        for i in 0..1_000 {
            w.observe_document(&doc! { "x": i as i64 });
        }
        let cols = w.into_columns();
        let x = col(&cols, "x");
        assert!(x.sample_values.as_ref().unwrap().len() <= SAMPLE_VALUES_PER_PATH);
    }

    #[test]
    fn max_depth_truncation() {
        // Build a 20-level deep nested document.
        let mut value = Bson::Int32(1);
        for _ in 0..20 {
            let mut d = Document::new();
            d.insert("n", value);
            value = Bson::Document(d);
        }
        let mut top = Document::new();
        top.insert("root", value);

        let mut w = SchemaWalker::new();
        w.observe_document(&top);
        let truncated = w.truncated_paths();
        let cols = w.into_columns();
        // Some leaf path should have been emitted with the truncated marker.
        assert!(cols.iter().any(|c| c.data_type == "<truncated>"));
        // The truncation counter must have ticked at least once.
        assert!(truncated >= 1);
    }

    #[test]
    fn max_paths_limit_drops_extras() {
        let mut w = SchemaWalker::new();
        let mut d = Document::new();
        for i in 0..(MAX_PATHS_PER_COLLECTION + 200) {
            d.insert(format!("f{}", i), Bson::Int32(i as i32));
        }
        w.observe_document(&d);
        let truncated = w.truncated_paths();
        let cols = w.into_columns();
        assert_eq!(cols.len(), MAX_PATHS_PER_COLLECTION);
        assert!(truncated >= 200);
    }

    #[test]
    fn pick_sample_size_clamps() {
        assert_eq!(pick_sample_size(0), MIN_SAMPLES);
        assert_eq!(pick_sample_size(50), 50);
        assert_eq!(pick_sample_size(MIN_SAMPLES), MIN_SAMPLES);
        // 1 % of 50_000 is 500, below MAX
        assert_eq!(pick_sample_size(50_000), 500);
        // 1 % of 1_000_000 would be 10_000 — clamps to MAX_SAMPLES
        assert_eq!(pick_sample_size(1_000_000), MAX_SAMPLES);
    }
}
