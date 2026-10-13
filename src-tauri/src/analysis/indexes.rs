//! Index analysis. Findings are observations about the catalog; the SQL produced here is a
//! suggestion to copy, never something DB-BUTURE runs.

use serde::Serialize;

use super::Evidence;
use crate::metadata::text::quote_ident;
use crate::metadata::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FkIndexGap {
    pub table: StableId,
    pub table_name: String,
    pub foreign_key: StableId,
    pub foreign_key_name: String,
    pub columns: Vec<String>,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RedundancyKind {
    /// Same columns, uniqueness, method and predicate.
    Duplicate,
    /// The index columns are a leading prefix of another index.
    Overlapping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRedundancy {
    pub kind: RedundancyKind,
    pub table: StableId,
    pub index: StableId,
    pub index_name: String,
    pub covered_by: StableId,
    pub covered_by_name: String,
    pub evidence: Evidence,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexSuggestion {
    pub title: String,
    pub table: StableId,
    pub sql: String,
    pub rationale: String,
    pub evidence: Evidence,
}

/// An index supports a FK lookup when its leading columns are exactly the FK columns (any order).
fn supports(index: &Index, fk_cols: &[String]) -> bool {
    if !index.expressions.is_empty() || index.columns.len() < fk_cols.len() {
        return false;
    }
    let mut lead: Vec<&String> = index.columns.iter().take(fk_cols.len()).collect();
    let mut want: Vec<&String> = fk_cols.iter().collect();
    lead.sort();
    want.sort();
    lead == want && index.predicate.is_none()
}

pub fn fk_without_index(schema: &DatabaseSchema) -> Vec<FkIndexGap> {
    let mut out = vec![];
    for t in &schema.tables {
        for fk in &t.foreign_keys {
            let pk_ok = t.primary_key.as_ref().is_some_and(|pk| {
                let idx = Index {
                    stable_id: pk.stable_id.clone(),
                    name: pk.name.clone(),
                    namespace: pk.namespace.clone(),
                    table: t.stable_id.clone(),
                    columns: pk.columns.clone(),
                    unique: true,
                    primary: true,
                    method: None,
                    predicate: None,
                    expressions: vec![],
                    included_columns: vec![],
                    definition: None,
                    engine_specific_metadata: EngineMeta::new(),
                };
                supports(&idx, &fk.source_columns)
            });
            let covered = pk_ok || t.indexes.iter().any(|i| supports(i, &fk.source_columns));
            if !covered {
                out.push(FkIndexGap {
                    table: t.stable_id.clone(),
                    table_name: t.name.clone(),
                    foreign_key: fk.stable_id.clone(),
                    foreign_key_name: fk.name.clone(),
                    columns: fk.source_columns.clone(),
                    evidence: Evidence::ObservedFact,
                });
            }
        }
    }
    out
}

pub fn redundant_indexes(schema: &DatabaseSchema) -> Vec<IndexRedundancy> {
    let mut out = vec![];
    for t in &schema.tables {
        let idx: Vec<&Index> = t
            .indexes
            .iter()
            .filter(|i| i.expressions.is_empty())
            .collect();
        for (n, a) in idx.iter().enumerate() {
            for (m, b) in idx.iter().enumerate() {
                // Covering (INCLUDE) indexes serve index-only scans: never call them redundant.
                if n == m || a.primary || !a.included_columns.is_empty() {
                    continue;
                }
                let same_shape = a.method == b.method && a.predicate == b.predicate;
                if !same_shape || a.columns.is_empty() {
                    continue;
                }
                if a.columns == b.columns
                    && a.unique == b.unique
                    && a.included_columns == b.included_columns
                {
                    // Report each duplicate pair once: the later id is the redundant one.
                    if a.stable_id > b.stable_id || b.primary {
                        out.push(entry(
                            t,
                            a,
                            b,
                            RedundancyKind::Duplicate,
                            format!("{} has exactly the same definition as {}", a.name, b.name),
                        ));
                    }
                } else if !a.unique
                    && b.columns.len() > a.columns.len()
                    && b.columns[..a.columns.len()] == a.columns[..]
                {
                    out.push(entry(
                        t,
                        a,
                        b,
                        RedundancyKind::Overlapping,
                        format!("columns of {} are a leading prefix of {}", a.name, b.name),
                    ));
                }
            }
        }
    }
    out.sort_by(|a, b| (&a.index, &a.covered_by).cmp(&(&b.index, &b.covered_by)));
    out.dedup_by(|a, b| a.index == b.index && a.covered_by == b.covered_by);
    out
}

fn entry(t: &Table, a: &Index, b: &Index, kind: RedundancyKind, why: String) -> IndexRedundancy {
    IndexRedundancy {
        kind,
        table: t.stable_id.clone(),
        index: a.stable_id.clone(),
        index_name: a.name.clone(),
        covered_by: b.stable_id.clone(),
        covered_by_name: b.name.clone(),
        // Identical definitions are a fact; overlap only suggests redundancy (the narrower
        // index can still serve ordering/covering needs or enforce nothing else).
        evidence: if kind == RedundancyKind::Duplicate {
            Evidence::ObservedFact
        } else {
            Evidence::Heuristic
        },
        explanation: why,
    }
}

fn index_name(table: &str, cols: &[String]) -> String {
    let mut n = format!("idx_{}_{}", table, cols.join("_"));
    n = n
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    n.chars().take(63).collect()
}

/// SQL is generated from catalog identifiers only, always quoted and escaped.
pub fn suggestions(schema: &DatabaseSchema) -> Vec<IndexSuggestion> {
    let e = schema.engine;
    let mut out = vec![];
    for gap in fk_without_index(schema) {
        let Some(t) = schema.tables.iter().find(|t| t.stable_id == gap.table) else {
            continue;
        };
        let cols: Vec<String> = gap.columns.iter().map(|c| quote_ident(e, c)).collect();
        let target = match e {
            Engine::Postgres => format!(
                "{}.{}",
                quote_ident(e, &t.namespace),
                quote_ident(e, &t.name)
            ),
            _ => quote_ident(e, &t.name),
        };
        let concurrently = if e == Engine::Postgres {
            " CONCURRENTLY"
        } else {
            ""
        };
        out.push(IndexSuggestion {
            title: format!("Index for foreign key {}", gap.foreign_key_name),
            table: gap.table.clone(),
            sql: format!(
                "CREATE INDEX{concurrently} {} ON {target} ({});",
                quote_ident(e, &index_name(&t.name, &gap.columns)),
                cols.join(", ")
            ),
            rationale:
                "No index starts with these foreign key columns. Joins and cascading deletes \
                        on the referenced table may scan this table. Verify with your workload."
                    .into(),
            evidence: Evidence::Suggestion,
        });
    }
    for r in redundant_indexes(schema) {
        let Some(t) = schema.tables.iter().find(|t| t.stable_id == r.table) else {
            continue;
        };
        let name = match e {
            Engine::Mysql => format!(
                "{} ON {}",
                quote_ident(e, &r.index_name),
                quote_ident(e, &t.name)
            ),
            Engine::Postgres => format!(
                "{}.{}",
                quote_ident(e, &t.namespace),
                quote_ident(e, &r.index_name)
            ),
            _ => quote_ident(e, &r.index_name),
        };
        let concurrently = if e == Engine::Postgres {
            " CONCURRENTLY"
        } else {
            ""
        };
        out.push(IndexSuggestion {
            title: format!("Review possibly redundant index {}", r.index_name),
            table: r.table.clone(),
            sql: format!("DROP INDEX{concurrently} {name};"),
            rationale: format!("{}. Check usage statistics before dropping.", r.explanation),
            evidence: Evidence::Suggestion,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_support::*;

    fn idx(table: &str, name: &str, cols: &[&str]) -> Index {
        Index {
            stable_id: StableId::object(Engine::Postgres, "index", "public", name),
            name: name.into(),
            namespace: "public".into(),
            table: tid(table),
            columns: cols.iter().map(|s| s.to_string()).collect(),
            unique: false,
            primary: false,
            method: Some("btree".into()),
            predicate: None,
            expressions: vec![],
            included_columns: vec![],
            definition: None,
            engine_specific_metadata: EngineMeta::new(),
        }
    }

    fn schema_with_fk() -> DatabaseSchema {
        let mut s = sample();
        let orders = s.tables.iter_mut().find(|t| t.name == "orders").unwrap();
        orders.foreign_keys.push(ForeignKey {
            stable_id: StableId::child(
                Engine::Postgres,
                "constraint",
                "public",
                "orders",
                "orders_user_fk",
            ),
            name: "orders_user_fk".into(),
            namespace: "public".into(),
            source_table: tid("orders"),
            source_columns: vec!["user_id".into()],
            target_table: tid("users"),
            target_columns: vec!["id".into()],
            on_delete: ReferentialAction::NoAction,
            on_update: ReferentialAction::NoAction,
            engine_specific_metadata: EngineMeta::new(),
        });
        s
    }

    #[test]
    fn detects_fk_without_index_and_suggests_sql() {
        let s = schema_with_fk();
        let gaps = fk_without_index(&s);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].evidence, Evidence::ObservedFact);
        let sug = suggestions(&s);
        assert_eq!(
            sug[0].sql,
            "CREATE INDEX CONCURRENTLY \"idx_orders_user_id\" ON \"public\".\"orders\" (\"user_id\");"
        );
        assert_eq!(sug[0].evidence, Evidence::Suggestion);
    }

    #[test]
    fn supporting_index_clears_the_gap_including_composite_prefix() {
        let mut s = schema_with_fk();
        s.tables
            .iter_mut()
            .find(|t| t.name == "orders")
            .unwrap()
            .indexes
            .push(idx("orders", "i1", &["user_id", "created_at"]));
        assert!(fk_without_index(&s).is_empty());
    }

    #[test]
    fn non_leading_or_partial_index_does_not_support_fk() {
        let mut s = schema_with_fk();
        let t = s.tables.iter_mut().find(|t| t.name == "orders").unwrap();
        t.indexes
            .push(idx("orders", "i1", &["created_at", "user_id"]));
        let mut partial = idx("orders", "i2", &["user_id"]);
        partial.predicate = Some("(deleted = false)".into());
        t.indexes.push(partial);
        assert_eq!(fk_without_index(&s).len(), 1);
    }

    #[test]
    fn detects_duplicate_and_overlapping_indexes() {
        let mut s = sample();
        let t = s.tables.iter_mut().find(|t| t.name == "orders").unwrap();
        t.indexes.push(idx("orders", "a", &["x"]));
        t.indexes.push(idx("orders", "b", &["x"]));
        t.indexes.push(idx("orders", "c", &["x", "y"]));
        let r = redundant_indexes(&s);
        assert!(r.iter().any(|x| x.kind == RedundancyKind::Duplicate));
        assert!(r
            .iter()
            .any(|x| x.kind == RedundancyKind::Overlapping && x.evidence == Evidence::Heuristic));
        let dup = r
            .iter()
            .filter(|x| x.kind == RedundancyKind::Duplicate)
            .count();
        assert_eq!(dup, 1, "each duplicate pair is reported once");
    }

    #[test]
    fn covering_indexes_are_not_flagged_as_overlapping() {
        let mut s = sample();
        let t = s.tables.iter_mut().find(|t| t.name == "orders").unwrap();
        let mut cover = idx("orders", "cover", &["x"]);
        cover.included_columns = vec!["payload".into()];
        t.indexes.push(cover);
        t.indexes.push(idx("orders", "wide", &["x", "y"]));
        assert!(redundant_indexes(&s).is_empty());
    }

    #[test]
    fn unique_and_primary_indexes_are_never_called_overlapping() {
        let mut s = sample();
        let t = s.tables.iter_mut().find(|t| t.name == "orders").unwrap();
        let mut u = idx("orders", "u", &["x"]);
        u.unique = true;
        t.indexes.push(u);
        t.indexes.push(idx("orders", "c", &["x", "y"]));
        assert!(redundant_indexes(&s).is_empty());
    }

    #[test]
    fn generated_sql_escapes_hostile_identifiers() {
        let mut s = schema_with_fk();
        let t = s.tables.iter_mut().find(|t| t.name == "orders").unwrap();
        t.foreign_keys[0].source_columns = vec!["a\"; DROP TABLE users; --".into()];
        let sql = &suggestions(&s)[0].sql;
        assert!(sql.contains("\"a\"\"; DROP TABLE users; --\""));
    }
}
