//! Deterministic schema snapshots. Volatile statistics (row estimates, sizes) are excluded so
//! two introspections of an unchanged schema always hash identically.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::metadata::DatabaseSchema;

/// Canonical JSON: object keys sorted, volatile fields removed.
pub fn canonical_json(schema: &DatabaseSchema) -> String {
    let mut v = serde_json::to_value(schema).expect("schema is serializable");
    strip_volatile(&mut v);
    serde_json::to_string(&sort_keys(v)).expect("value is serializable")
}

pub fn sha256_hex(schema: &DatabaseSchema) -> String {
    hex::encode(Sha256::digest(canonical_json(schema).as_bytes()))
}

fn strip_volatile(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("estimatedRows");
            m.remove("sizeBytes");
            m.remove("serverVersion");
            m.values_mut().for_each(strip_volatile);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_volatile),
        _ => {}
    }
}

fn sort_keys(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let sorted: std::collections::BTreeMap<String, Value> =
                m.into_iter().map(|(k, v)| (k, sort_keys(v))).collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub id: String,
    pub name: String,
    pub connection_id: Option<String>,
    pub sha256: String,
    pub created_at: String,
    pub tables: usize,
    pub views: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_support::sample;

    #[test]
    fn hash_is_stable_and_hex_sha256() {
        let s = sample();
        let h = sha256_hex(&s);
        assert_eq!(h.len(), 64);
        assert_eq!(h, sha256_hex(&s.clone()));
    }

    #[test]
    fn hash_ignores_volatile_stats_but_not_structure() {
        let a = sample();
        let mut b = a.clone();
        b.tables[0].estimated_rows = Some(999);
        b.tables[0].size_bytes = Some(1);
        b.server_version = Some("99".into());
        assert_eq!(sha256_hex(&a), sha256_hex(&b));
        b.tables[0].comment = Some("changed".into());
        assert_ne!(sha256_hex(&a), sha256_hex(&b));
    }

    #[test]
    fn canonical_json_has_sorted_keys() {
        let j = canonical_json(&sample());
        let engine = j.find("\"engine\"").unwrap();
        let tables = j.find("\"tables\"").unwrap();
        assert!(engine < tables);
        assert!(!j.contains("estimatedRows"));
    }
}
