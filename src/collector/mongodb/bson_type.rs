//! BSON value → stable type label mapping.
//!
//! Used by both schema inference (recording which types are observed at each
//! path) and the resulting `ColumnMetadata.data_type` field. The labels are
//! lowercase, snake-cased, and stable across runs so consumers can rely on
//! them for migration mapping decisions.

use mongodb::bson::Bson;

/// Map a BSON value to a stable lowercase type label.
///
/// The label set mirrors `bson::spec::ElementType` but is exposed as
/// `&'static str` so callers can store it in `BTreeSet<&'static str>` without
/// allocating per observation.
pub fn bson_type_label(value: &Bson) -> &'static str {
    match value {
        Bson::Double(_) => "double",
        Bson::String(_) => "string",
        Bson::Document(_) => "document",
        Bson::Array(_) => "array",
        Bson::Binary(_) => "binary",
        Bson::Undefined => "undefined",
        Bson::ObjectId(_) => "object_id",
        Bson::Boolean(_) => "bool",
        Bson::DateTime(_) => "date_time",
        Bson::Null => "null",
        Bson::RegularExpression(_) => "regex",
        Bson::JavaScriptCode(_) => "javascript",
        Bson::JavaScriptCodeWithScope(_) => "javascript",
        Bson::Symbol(_) => "symbol",
        Bson::Int32(_) => "int32",
        Bson::Int64(_) => "int64",
        Bson::Timestamp(_) => "timestamp",
        Bson::Decimal128(_) => "decimal128",
        Bson::DbPointer(_) => "db_pointer",
        Bson::MaxKey => "max_key",
        Bson::MinKey => "min_key",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::{doc, oid::ObjectId, Bson, DateTime, Decimal128, Timestamp};

    #[test]
    fn label_scalars() {
        assert_eq!(bson_type_label(&Bson::Double(1.0)), "double");
        assert_eq!(bson_type_label(&Bson::String("x".into())), "string");
        assert_eq!(bson_type_label(&Bson::Boolean(true)), "bool");
        assert_eq!(bson_type_label(&Bson::Null), "null");
        assert_eq!(bson_type_label(&Bson::Int32(42)), "int32");
        assert_eq!(bson_type_label(&Bson::Int64(42)), "int64");
    }

    #[test]
    fn label_containers() {
        assert_eq!(bson_type_label(&Bson::Document(doc! {})), "document");
        assert_eq!(bson_type_label(&Bson::Array(vec![])), "array");
    }

    #[test]
    fn label_special_types() {
        assert_eq!(
            bson_type_label(&Bson::ObjectId(ObjectId::new())),
            "object_id"
        );
        assert_eq!(
            bson_type_label(&Bson::DateTime(DateTime::now())),
            "date_time"
        );
        assert_eq!(
            bson_type_label(&Bson::Timestamp(Timestamp {
                time: 0,
                increment: 0
            })),
            "timestamp"
        );
        let dec: Decimal128 = "1.5".parse().unwrap();
        assert_eq!(bson_type_label(&Bson::Decimal128(dec)), "decimal128");
        assert_eq!(bson_type_label(&Bson::MinKey), "min_key");
        assert_eq!(bson_type_label(&Bson::MaxKey), "max_key");
        assert_eq!(bson_type_label(&Bson::Undefined), "undefined");
    }
}
