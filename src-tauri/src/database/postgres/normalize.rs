//! Raw PostgreSQL catalog rows -> `DatabaseSchema`.
//!
//! Output is sorted deterministically and validated: foreign keys pointing at
//! objects that were not introspected (hidden system schemas) are dropped and
//! reported as warnings instead of producing dangling graph edges.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::json;

use super::raw::*;
use crate::metadata::types::normalize_type;
use crate::metadata::*;

const E: Engine = Engine::Postgres;

pub fn normalize(raw: RawCatalog) -> DatabaseSchema {
    let mut schema = DatabaseSchema::empty(E, raw.database.clone());
    schema.server_version = Some(raw.version.clone());

    schema.namespaces = raw
        .namespaces
        .iter()
        .map(|n| DatabaseNamespace {
            stable_id: StableId::namespace(E, &n.name),
            name: n.name.clone(),
            is_system: n.is_system,
            engine_specific_metadata: EngineMeta::new(),
        })
        .collect();

    let mut columns_by_rel: HashMap<(&str, &str), Vec<Column>> = HashMap::new();
    for c in &raw.columns {
        let mut meta = EngineMeta::new();
        if let Some(g) = &c.generated {
            meta.insert("generationKind".into(), json!(g));
        }
        columns_by_rel
            .entry((c.namespace.as_str(), c.relation.as_str()))
            .or_default()
            .push(Column {
                stable_id: StableId::child(E, "column", &c.namespace, &c.relation, &c.name),
                name: c.name.clone(),
                namespace: c.namespace.clone(),
                ordinal: c.ordinal,
                data_type: normalize_type(E, &c.native_type),
                native_type: c.native_type.clone(),
                nullable: !c.not_null,
                default: c.default.clone(),
                generated: c.generated.as_ref().map(|_| "stored".to_string()),
                identity: c.identity.as_deref().map(|i| match i {
                    "a" => "always".to_string(),
                    _ => "by default".to_string(),
                }),
                comment: c.comment.clone(),
                engine_specific_metadata: meta,
            });
    }

    let table_ids: HashSet<StableId> = raw
        .relations
        .iter()
        .filter(|r| matches!(r.kind.as_str(), "r" | "p"))
        .map(|r| StableId::table(E, &r.namespace, &r.name))
        .collect();

    for rel in &raw.relations {
        let cols = columns_by_rel
            .get(&(rel.namespace.as_str(), rel.name.as_str()))
            .cloned()
            .unwrap_or_default();
        match rel.kind.as_str() {
            "r" | "p" => {
                let mut meta = EngineMeta::new();
                meta.insert("partitioned".into(), json!(rel.kind == "p"));
                meta.insert("isPartition".into(), json!(rel.is_partition));
                schema.tables.push(Table {
                    stable_id: StableId::table(E, &rel.namespace, &rel.name),
                    name: rel.name.clone(),
                    namespace: rel.namespace.clone(),
                    comment: rel.comment.clone(),
                    // reltuples is -1 for never analyzed tables: that means unknown.
                    estimated_rows: rel.estimated_rows.filter(|n| *n >= 0),
                    size_bytes: rel.size_bytes,
                    columns: cols,
                    primary_key: None,
                    foreign_keys: vec![],
                    unique_constraints: vec![],
                    check_constraints: vec![],
                    indexes: vec![],
                    engine_specific_metadata: meta,
                });
            }
            "v" | "m" => schema.views.push(View {
                stable_id: StableId::object(
                    E,
                    if rel.kind == "m" { "matview" } else { "view" },
                    &rel.namespace,
                    &rel.name,
                ),
                name: rel.name.clone(),
                namespace: rel.namespace.clone(),
                materialized: rel.kind == "m",
                definition: rel.view_definition.clone(),
                columns: cols,
                comment: rel.comment.clone(),
                engine_specific_metadata: EngineMeta::new(),
            }),
            _ => {}
        }
    }

    let mut table_pos: HashMap<StableId, usize> = HashMap::new();
    for (i, t) in schema.tables.iter().enumerate() {
        table_pos.insert(t.stable_id.clone(), i);
    }

    for c in &raw.constraints {
        let tid = StableId::table(E, &c.namespace, &c.relation);
        let Some(&pos) = table_pos.get(&tid) else {
            continue;
        };
        let sid = StableId::child(E, "constraint", &c.namespace, &c.relation, &c.name);
        let mut meta = EngineMeta::new();
        meta.insert("definition".into(), json!(c.definition));
        meta.insert("deferrable".into(), json!(c.deferrable));
        meta.insert("validated".into(), json!(c.validated));
        let table = &mut schema.tables[pos];
        match c.kind.as_str() {
            "p" => {
                table.primary_key = Some(PrimaryKey {
                    stable_id: sid,
                    name: c.name.clone(),
                    namespace: c.namespace.clone(),
                    columns: c.columns.clone(),
                    engine_specific_metadata: meta,
                })
            }
            "u" => table.unique_constraints.push(UniqueConstraint {
                stable_id: sid,
                name: c.name.clone(),
                namespace: c.namespace.clone(),
                columns: c.columns.clone(),
                engine_specific_metadata: meta,
            }),
            "c" => table.check_constraints.push(CheckConstraint {
                stable_id: sid,
                name: c.name.clone(),
                namespace: c.namespace.clone(),
                expression: c.definition.clone(),
                engine_specific_metadata: meta,
            }),
            "f" => {
                let (Some(rns), Some(rrel), Some(rcols)) =
                    (&c.ref_namespace, &c.ref_relation, &c.ref_columns)
                else {
                    schema
                        .warnings
                        .push(format!("foreign key {} has no target", c.name));
                    continue;
                };
                let target = StableId::table(E, rns, rrel);
                if !table_ids.contains(&target) {
                    schema.warnings.push(format!(
                        "foreign key {}.{}.{} references {}.{} which was not introspected",
                        c.namespace, c.relation, c.name, rns, rrel
                    ));
                    continue;
                }
                table.foreign_keys.push(ForeignKey {
                    stable_id: sid,
                    name: c.name.clone(),
                    namespace: c.namespace.clone(),
                    source_table: tid,
                    source_columns: c.columns.clone(),
                    target_table: target,
                    target_columns: rcols.clone(),
                    on_delete: action(c.on_delete.as_deref()),
                    on_update: action(c.on_update.as_deref()),
                    engine_specific_metadata: meta,
                });
            }
            _ => {}
        }
    }

    for i in &raw.indexes {
        let tid = StableId::table(E, &i.namespace, &i.table);
        let Some(&pos) = table_pos.get(&tid) else {
            continue;
        };
        let mut columns = vec![];
        let mut expressions = vec![];
        for (entry, is_expr) in i.key_entries.iter().zip(&i.key_is_expr) {
            if *is_expr {
                expressions.push(entry.clone());
            } else {
                columns.push(entry.clone());
            }
        }
        let mut meta = EngineMeta::new();
        meta.insert("valid".into(), json!(i.is_valid));
        schema.tables[pos].indexes.push(Index {
            stable_id: StableId::object(E, "index", &i.namespace, &i.name),
            name: i.name.clone(),
            namespace: i.namespace.clone(),
            table: tid,
            columns,
            unique: i.is_unique,
            primary: i.is_primary,
            method: Some(i.method.clone()),
            predicate: i.predicate.clone(),
            expressions,
            included_columns: i.included.clone(),
            definition: Some(i.definition.clone()),
            engine_specific_metadata: meta,
        });
    }

    for r in &raw.routines {
        schema.routines.push(Routine {
            stable_id: StableId::routine(E, &r.namespace, &r.name, &r.arguments),
            name: r.name.clone(),
            namespace: r.namespace.clone(),
            kind: if r.is_procedure {
                RoutineKind::Procedure
            } else {
                RoutineKind::Function
            },
            language: Some(r.language.clone()),
            arguments: r.arguments.clone(),
            return_type: r.return_type.clone(),
            definition: r.definition.clone(),
            engine_specific_metadata: EngineMeta::new(),
        });
    }
    let routine_ids: HashSet<StableId> = schema
        .routines
        .iter()
        .map(|r| r.stable_id.clone())
        .collect();

    for s in &raw.sequences {
        schema.sequences.push(Sequence {
            stable_id: StableId::object(E, "sequence", &s.namespace, &s.name),
            name: s.name.clone(),
            namespace: s.namespace.clone(),
            data_type: Some(s.data_type.clone()),
            owned_by: match (&s.owner_namespace, &s.owner_table, &s.owner_column) {
                (Some(ns), Some(t), Some(c)) => Some(StableId::child(E, "column", ns, t, c)),
                _ => None,
            },
            engine_specific_metadata: EngineMeta::new(),
        });
    }

    for t in &raw.triggers {
        let table = StableId::table(E, &t.namespace, &t.table);
        if !table_pos.contains_key(&table)
            && !schema
                .views
                .iter()
                .any(|v| v.name == t.table && v.namespace == t.namespace)
        {
            continue;
        }
        let function =
            StableId::routine(E, &t.function_namespace, &t.function_name, &t.function_args);
        schema.triggers.push(Trigger {
            stable_id: StableId::child(E, "trigger", &t.namespace, &t.table, &t.name),
            name: t.name.clone(),
            namespace: t.namespace.clone(),
            table,
            timing: t.timing.clone(),
            events: t.events.clone(),
            definition: Some(t.definition.clone()),
            function: routine_ids.contains(&function).then_some(function),
            engine_specific_metadata: EngineMeta::new(),
        });
    }

    // Dependencies
    let view_ids: HashMap<(&str, &str), StableId> = schema
        .views
        .iter()
        .map(|v| ((v.namespace.as_str(), v.name.as_str()), v.stable_id.clone()))
        .collect();
    let mut deps: Vec<Dependency> = vec![];
    for t in &schema.tables {
        for fk in &t.foreign_keys {
            deps.push(Dependency {
                from: fk.source_table.clone(),
                to: fk.target_table.clone(),
                kind: DependencyKind::ForeignKey,
            });
        }
    }
    let resolve = |ns: &str, name: &str| -> Option<StableId> {
        let tid = StableId::table(E, ns, name);
        if table_ids.contains(&tid) {
            Some(tid)
        } else {
            view_ids.get(&(ns, name)).cloned()
        }
    };
    for d in &raw.view_dependencies {
        let (Some(from), Some(to)) = (
            view_ids
                .get(&(d.view_namespace.as_str(), d.view_name.as_str()))
                .cloned(),
            resolve(&d.source_namespace, &d.source_name),
        ) else {
            continue;
        };
        deps.push(Dependency {
            from,
            to,
            kind: DependencyKind::DependsOn,
        });
    }
    for tr in &schema.triggers {
        deps.push(Dependency {
            from: tr.stable_id.clone(),
            to: tr.table.clone(),
            kind: DependencyKind::TriggerDependency,
        });
        if let Some(f) = &tr.function {
            deps.push(Dependency {
                from: tr.stable_id.clone(),
                to: f.clone(),
                kind: DependencyKind::TriggerDependency,
            });
        }
    }
    deps.sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
    deps.dedup();
    schema.dependencies = deps;

    sort_schema(&mut schema);
    schema
}

fn action(code: Option<&str>) -> ReferentialAction {
    match code {
        Some("r") => ReferentialAction::Restrict,
        Some("c") => ReferentialAction::Cascade,
        Some("n") => ReferentialAction::SetNull,
        Some("d") => ReferentialAction::SetDefault,
        _ => ReferentialAction::NoAction,
    }
}

/// Deterministic ordering independent of query result order.
pub fn sort_schema(schema: &mut DatabaseSchema) {
    schema
        .namespaces
        .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    schema.tables.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    schema.views.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    schema
        .triggers
        .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    schema
        .routines
        .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    schema
        .sequences
        .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    for t in &mut schema.tables {
        t.columns.sort_by_key(|c| c.ordinal);
        t.foreign_keys.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.unique_constraints
            .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.check_constraints
            .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.indexes.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    }
    for v in &mut schema.views {
        v.columns.sort_by_key(|c| c.ordinal);
    }
    schema.warnings.sort();
}

/// Structural validation used by tests and callers that want to assert integrity.
pub fn validate(schema: &DatabaseSchema) -> Result<(), String> {
    let mut seen: BTreeMap<&StableId, ()> = BTreeMap::new();
    for t in &schema.tables {
        if seen.insert(&t.stable_id, ()).is_some() {
            return Err(format!("duplicate id {}", t.stable_id));
        }
        for fk in &t.foreign_keys {
            if fk.source_columns.len() != fk.target_columns.len() {
                return Err(format!("fk {} column count mismatch", fk.name));
            }
            if !schema.tables.iter().any(|x| x.stable_id == fk.target_table) {
                return Err(format!("fk {} targets unknown table", fk.name));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(ns: &str, rel: &str, name: &str, ord: i32, ty: &str, not_null: bool) -> RawColumn {
        RawColumn {
            namespace: ns.into(),
            relation: rel.into(),
            name: name.into(),
            ordinal: ord,
            native_type: ty.into(),
            not_null,
            default: None,
            identity: None,
            generated: None,
            comment: None,
        }
    }

    fn rel(ns: &str, name: &str, kind: &str) -> RawRelation {
        RawRelation {
            namespace: ns.into(),
            name: name.into(),
            kind: kind.into(),
            comment: None,
            estimated_rows: Some(10),
            size_bytes: Some(8192),
            view_definition: (kind == "v").then(|| "SELECT 1".to_string()),
            is_partition: false,
        }
    }

    fn fk(name: &str, from: &str, cols: &[&str], to: &str, tcols: &[&str]) -> RawConstraint {
        RawConstraint {
            namespace: "public".into(),
            relation: from.into(),
            name: name.into(),
            kind: "f".into(),
            columns: cols.iter().map(|s| s.to_string()).collect(),
            ref_namespace: Some("public".into()),
            ref_relation: Some(to.into()),
            ref_columns: Some(tcols.iter().map(|s| s.to_string()).collect()),
            on_update: Some("a".into()),
            on_delete: Some("c".into()),
            definition: "FOREIGN KEY ...".into(),
            deferrable: false,
            validated: true,
        }
    }

    fn catalog() -> RawCatalog {
        RawCatalog {
            version: "17.2".into(),
            database: "app".into(),
            namespaces: vec![RawNamespace {
                name: "public".into(),
                is_system: false,
            }],
            relations: vec![
                rel("public", "orders", "r"),
                rel("public", "users", "r"),
                rel("public", "order_items", "r"),
                rel("public", "user_orders", "v"),
            ],
            columns: vec![
                col(
                    "public",
                    "users",
                    "email",
                    2,
                    "character varying(255)",
                    true,
                ),
                col("public", "users", "id", 1, "bigint", true),
                col("public", "orders", "id", 1, "bigint", true),
                col("public", "orders", "user_id", 2, "bigint", false),
                col("public", "order_items", "order_id", 1, "bigint", true),
                col("public", "order_items", "line", 2, "integer", true),
            ],
            constraints: vec![
                RawConstraint {
                    namespace: "public".into(),
                    relation: "order_items".into(),
                    name: "order_items_pkey".into(),
                    kind: "p".into(),
                    columns: vec!["order_id".into(), "line".into()],
                    ref_namespace: None,
                    ref_relation: None,
                    ref_columns: None,
                    on_update: None,
                    on_delete: None,
                    definition: "PRIMARY KEY (order_id, line)".into(),
                    deferrable: false,
                    validated: true,
                },
                fk("orders_user_fk", "orders", &["user_id"], "users", &["id"]),
                fk(
                    "items_order_fk",
                    "order_items",
                    &["order_id"],
                    "orders",
                    &["id"],
                ),
            ],
            view_dependencies: vec![RawViewDependency {
                view_namespace: "public".into(),
                view_name: "user_orders".into(),
                source_namespace: "public".into(),
                source_name: "orders".into(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn builds_tables_columns_and_composite_pk() {
        let s = normalize(catalog());
        assert_eq!(s.tables.len(), 3);
        let items = s.tables.iter().find(|t| t.name == "order_items").unwrap();
        assert_eq!(
            items.primary_key.as_ref().unwrap().columns,
            ["order_id", "line"]
        );
        let users = s.tables.iter().find(|t| t.name == "users").unwrap();
        assert_eq!(users.columns[0].name, "id");
        assert_eq!(users.columns[1].data_type, "varchar");
        assert_eq!(users.columns[1].native_type, "character varying(255)");
    }

    #[test]
    fn foreign_keys_and_dependencies() {
        let s = normalize(catalog());
        let orders = s.tables.iter().find(|t| t.name == "orders").unwrap();
        assert_eq!(orders.foreign_keys[0].on_delete, ReferentialAction::Cascade);
        assert_eq!(
            s.dependencies
                .iter()
                .filter(|d| d.kind == DependencyKind::ForeignKey)
                .count(),
            2
        );
        assert!(s
            .dependencies
            .iter()
            .any(|d| d.kind == DependencyKind::DependsOn
                && d.from.as_str() == "postgres:view:public.user_orders"
                && d.to.as_str() == "postgres:public.orders"));
        validate(&s).unwrap();
    }

    #[test]
    fn composite_fk_keeps_all_columns() {
        let mut c = catalog();
        c.constraints.push(fk(
            "c",
            "order_items",
            &["order_id", "line"],
            "order_items",
            &["order_id", "line"],
        ));
        let s = normalize(c);
        let t = s.tables.iter().find(|t| t.name == "order_items").unwrap();
        let f = t.foreign_keys.iter().find(|f| f.name == "c").unwrap();
        assert_eq!(f.source_columns.len(), 2);
        assert_eq!(f.target_columns.len(), 2);
    }

    #[test]
    fn output_is_independent_of_input_order() {
        let a = normalize(catalog());
        let mut c = catalog();
        c.relations.reverse();
        c.columns.reverse();
        c.constraints.reverse();
        let b = normalize(c);
        assert_eq!(a, b);
    }

    #[test]
    fn dangling_fk_is_dropped_with_warning() {
        let mut c = catalog();
        c.constraints
            .push(fk("ghost", "orders", &["user_id"], "missing", &["id"]));
        let s = normalize(c);
        assert!(s.warnings.iter().any(|w| w.contains("ghost")));
        validate(&s).unwrap();
    }

    #[test]
    fn unknown_row_estimate_is_none() {
        let mut c = catalog();
        c.relations[0].estimated_rows = Some(-1);
        let s = normalize(c);
        assert_eq!(
            s.tables
                .iter()
                .find(|t| t.name == "orders")
                .unwrap()
                .estimated_rows,
            None
        );
    }

    #[test]
    fn expression_indexes_are_separated_from_columns() {
        let mut c = catalog();
        c.indexes.push(RawIndex {
            namespace: "public".into(),
            table: "users".into(),
            name: "idx_users_lower_email".into(),
            method: "btree".into(),
            is_unique: true,
            is_primary: false,
            is_valid: true,
            key_entries: vec!["lower((email)::text)".into(), "id".into()],
            key_is_expr: vec![true, false],
            included: vec![],
            predicate: Some("(id > 0)".into()),
            definition: "CREATE UNIQUE INDEX ...".into(),
        });
        let s = normalize(c);
        let idx = &s.tables.iter().find(|t| t.name == "users").unwrap().indexes[0];
        assert_eq!(idx.columns, ["id"]);
        assert_eq!(idx.expressions, ["lower((email)::text)"]);
        assert_eq!(idx.predicate.as_deref(), Some("(id > 0)"));
    }
}
