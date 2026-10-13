//! Structural diff between two normalized schemas, matched by stable ids.
//! Row estimates and sizes are statistics, not structure, and are never diffed.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Value};

use crate::metadata::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    pub field: String,
    pub before: Value,
    pub after: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    pub id: StableId,
    pub object_type: &'static str,
    pub name: String,
    /// Owning table/view for children (columns, constraints, indexes, triggers).
    pub parent: Option<StableId>,
    pub change: ChangeKind,
    pub fields: Vec<FieldChange>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiff {
    pub entries: Vec<DiffEntry>,
    pub summary: DiffSummary,
}

struct Obj {
    object_type: &'static str,
    name: String,
    parent: Option<StableId>,
    props: Value,
}

fn collect(s: &DatabaseSchema) -> BTreeMap<StableId, Obj> {
    let mut m = BTreeMap::new();
    let mut put =
        |id: &StableId, ty: &'static str, name: &str, parent: Option<&StableId>, props: Value| {
            m.insert(
                id.clone(),
                Obj {
                    object_type: ty,
                    name: name.into(),
                    parent: parent.cloned(),
                    props,
                },
            );
        };
    for t in &s.tables {
        put(
            &t.stable_id,
            "table",
            &t.name,
            None,
            json!({ "comment": t.comment }),
        );
        for c in &t.columns {
            put(
                &c.stable_id,
                "column",
                &c.name,
                Some(&t.stable_id),
                json!({
                    "type": c.native_type, "nullable": c.nullable, "default": c.default,
                    "generated": c.generated, "identity": c.identity, "comment": c.comment,
                }),
            );
        }
        if let Some(pk) = &t.primary_key {
            put(
                &pk.stable_id,
                "primary_key",
                &pk.name,
                Some(&t.stable_id),
                json!({ "columns": pk.columns }),
            );
        }
        for f in &t.foreign_keys {
            put(
                &f.stable_id,
                "foreign_key",
                &f.name,
                Some(&t.stable_id),
                json!({
                    "columns": f.source_columns, "references": f.target_table, "targetColumns": f.target_columns,
                    "onDelete": f.on_delete, "onUpdate": f.on_update,
                }),
            );
        }
        for u in &t.unique_constraints {
            put(
                &u.stable_id,
                "unique",
                &u.name,
                Some(&t.stable_id),
                json!({ "columns": u.columns }),
            );
        }
        for c in &t.check_constraints {
            put(
                &c.stable_id,
                "check",
                &c.name,
                Some(&t.stable_id),
                json!({ "expression": c.expression }),
            );
        }
        for i in &t.indexes {
            put(
                &i.stable_id,
                "index",
                &i.name,
                Some(&t.stable_id),
                json!({
                    "columns": i.columns, "unique": i.unique, "method": i.method, "predicate": i.predicate,
                    "expressions": i.expressions, "included": i.included_columns,
                }),
            );
        }
    }
    for v in &s.views {
        put(
            &v.stable_id,
            if v.materialized {
                "materialized_view"
            } else {
                "view"
            },
            &v.name,
            None,
            json!({ "definition": v.definition }),
        );
        for c in &v.columns {
            put(
                &c.stable_id,
                "column",
                &c.name,
                Some(&v.stable_id),
                json!({ "type": c.native_type, "nullable": c.nullable }),
            );
        }
    }
    for t in &s.triggers {
        put(
            &t.stable_id,
            "trigger",
            &t.name,
            Some(&t.table),
            json!({
                "timing": t.timing, "events": t.events, "definition": t.definition,
            }),
        );
    }
    for r in &s.routines {
        put(
            &r.stable_id,
            "routine",
            &r.name,
            None,
            json!({
                "returns": r.return_type, "language": r.language, "definition": r.definition,
            }),
        );
    }
    for q in &s.sequences {
        put(
            &q.stable_id,
            "sequence",
            &q.name,
            None,
            json!({ "type": q.data_type, "ownedBy": q.owned_by }),
        );
    }
    m
}

pub fn diff_schemas(before: &DatabaseSchema, after: &DatabaseSchema) -> SchemaDiff {
    let (a, b) = (collect(before), collect(after));
    let mut entries = vec![];
    for (id, o) in &a {
        match b.get(id) {
            None => entries.push(DiffEntry {
                id: id.clone(),
                object_type: o.object_type,
                name: o.name.clone(),
                parent: o.parent.clone(),
                change: ChangeKind::Removed,
                fields: vec![],
            }),
            Some(n) if n.props != o.props => {
                let mut fields = vec![];
                if let (Value::Object(x), Value::Object(y)) = (&o.props, &n.props) {
                    for (k, v) in x {
                        if y.get(k) != Some(v) {
                            fields.push(FieldChange {
                                field: k.clone(),
                                before: v.clone(),
                                after: y.get(k).cloned().unwrap_or(Value::Null),
                            });
                        }
                    }
                }
                entries.push(DiffEntry {
                    id: id.clone(),
                    object_type: o.object_type,
                    name: o.name.clone(),
                    parent: o.parent.clone(),
                    change: ChangeKind::Changed,
                    fields,
                });
            }
            _ => {}
        }
    }
    for (id, o) in &b {
        if !a.contains_key(id) {
            entries.push(DiffEntry {
                id: id.clone(),
                object_type: o.object_type,
                name: o.name.clone(),
                parent: o.parent.clone(),
                change: ChangeKind::Added,
                fields: vec![],
            });
        }
    }
    entries.sort_by(|x, y| x.id.cmp(&y.id));
    let summary = DiffSummary {
        added: entries
            .iter()
            .filter(|e| e.change == ChangeKind::Added)
            .count(),
        removed: entries
            .iter()
            .filter(|e| e.change == ChangeKind::Removed)
            .count(),
        changed: entries
            .iter()
            .filter(|e| e.change == ChangeKind::Changed)
            .count(),
    };
    SchemaDiff { entries, summary }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_support::*;

    fn col(t: &str, name: &str, ty: &str) -> Column {
        Column {
            stable_id: StableId::child(Engine::Postgres, "column", "public", t, name),
            name: name.into(),
            namespace: "public".into(),
            ordinal: 1,
            data_type: ty.into(),
            native_type: ty.into(),
            nullable: true,
            default: None,
            generated: None,
            identity: None,
            comment: None,
            engine_specific_metadata: EngineMeta::new(),
        }
    }

    #[test]
    fn identical_schemas_have_no_diff() {
        let d = diff_schemas(&sample(), &sample());
        assert!(d.entries.is_empty());
        assert_eq!(d.summary, DiffSummary::default());
    }

    #[test]
    fn statistics_are_not_structure() {
        let a = sample();
        let mut b = sample();
        b.tables[0].estimated_rows = Some(5);
        assert!(diff_schemas(&a, &b).entries.is_empty());
    }

    #[test]
    fn detects_added_removed_and_changed() {
        fn lonely(s: &mut DatabaseSchema) -> &mut Table {
            s.tables.iter_mut().find(|t| t.name == "lonely").unwrap()
        }
        let mut a = sample();
        lonely(&mut a).columns.push(col("lonely", "x", "integer"));
        lonely(&mut a).columns.push(col("lonely", "gone", "text"));
        let mut b = a.clone();
        lonely(&mut b).columns.retain(|c| c.name != "gone");
        lonely(&mut b)
            .columns
            .iter_mut()
            .find(|c| c.name == "x")
            .unwrap()
            .native_type = "bigint".into();
        lonely(&mut b).columns.push(col("lonely", "added", "text"));
        b.tables.push(table("brand_new"));

        let d = diff_schemas(&a, &b);
        assert_eq!(
            d.summary,
            DiffSummary {
                added: 2,
                removed: 1,
                changed: 1
            }
        );
        let changed = d
            .entries
            .iter()
            .find(|e| e.change == ChangeKind::Changed)
            .unwrap();
        assert_eq!(changed.fields[0].field, "type");
        assert_eq!(changed.fields[0].before, json!("integer"));
        assert_eq!(changed.fields[0].after, json!("bigint"));
        assert_eq!(changed.parent.as_ref().unwrap(), &tid("lonely"));
    }

    #[test]
    fn renamed_object_appears_as_remove_plus_add() {
        let a = sample();
        let mut b = sample();
        b.tables[0] = table("renamed");
        let d = diff_schemas(&a, &b);
        assert_eq!((d.summary.added, d.summary.removed), (1, 1));
    }
}
