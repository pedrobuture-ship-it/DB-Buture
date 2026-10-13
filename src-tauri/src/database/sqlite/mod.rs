//! SQLite adapter: sqlite_schema + PRAGMA table-valued functions (all parameter bound).

use std::collections::{BTreeMap, HashMap};

use serde_json::json;
use sqlx::{Row, SqlitePool};

use crate::error::{AppError, AppResult};
use crate::metadata::text::mentions_identifier as mentions;
use crate::metadata::types::normalize_type;
use crate::metadata::*;

const E: Engine = Engine::Sqlite;
const NS: &str = "main";

fn map(e: sqlx::Error) -> AppError {
    AppError::Introspection(e.to_string())
}

pub async fn introspect(pool: &SqlitePool, database_name: &str) -> AppResult<DatabaseSchema> {
    let mut schema = DatabaseSchema::empty(E, database_name);
    schema.server_version = Some(
        sqlx::query_scalar::<_, String>("SELECT sqlite_version()")
            .fetch_one(pool)
            .await
            .map_err(map)?,
    );
    schema.namespaces.push(DatabaseNamespace {
        stable_id: StableId::namespace(E, NS),
        name: NS.into(),
        is_system: false,
        engine_specific_metadata: EngineMeta::new(),
    });

    let objects = sqlx::query(
        "SELECT type, name, tbl_name, sql FROM sqlite_schema
         WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY type, name",
    )
    .fetch_all(pool)
    .await
    .map_err(map)?;

    struct Obj {
        kind: String,
        name: String,
        table: String,
        sql: Option<String>,
    }
    let objs: Vec<Obj> = objects
        .iter()
        .map(|r| Obj {
            kind: r.get("type"),
            name: r.get("name"),
            table: r.get("tbl_name"),
            sql: r.get("sql"),
        })
        .collect();

    let table_names: Vec<&str> = objs
        .iter()
        .filter(|o| o.kind == "table")
        .map(|o| o.name.as_str())
        .collect();

    // Columns + primary keys first: FKs may omit target columns (implicit PK reference).
    let mut pk_of: HashMap<String, Vec<String>> = HashMap::new();
    let mut tables: BTreeMap<String, Table> = BTreeMap::new();
    for obj in objs.iter().filter(|o| o.kind == "table") {
        let cols = sqlx::query(
            "SELECT cid, name, type, \"notnull\", dflt_value, pk, hidden FROM pragma_table_xinfo(?) ORDER BY cid",
        )
        .bind(&obj.name)
        .fetch_all(pool)
        .await
        .map_err(map)?;

        let mut columns = vec![];
        let mut pks: Vec<(i64, String)> = vec![];
        for (i, c) in cols.iter().enumerate() {
            let name: String = c.get("name");
            let native: String = c.get("type");
            let pk: i64 = c.get("pk");
            let hidden: i64 = c.get("hidden");
            if pk > 0 {
                pks.push((pk, name.clone()));
            }
            if hidden == 1 {
                continue; // virtual table hidden column
            }
            columns.push(Column {
                stable_id: StableId::child(E, "column", NS, &obj.name, &name),
                name,
                namespace: NS.into(),
                ordinal: (i + 1) as i32,
                data_type: if native.is_empty() {
                    "any".into()
                } else {
                    normalize_type(E, &native)
                },
                native_type: native,
                nullable: c.get::<i64, _>("notnull") == 0 && pk == 0,
                default: c.get("dflt_value"),
                generated: match hidden {
                    2 => Some("virtual".into()),
                    3 => Some("stored".into()),
                    _ => None,
                },
                identity: None,
                comment: None,
                engine_specific_metadata: EngineMeta::new(),
            });
        }
        pks.sort();
        let pk_cols: Vec<String> = pks.into_iter().map(|(_, n)| n).collect();
        pk_of.insert(obj.name.clone(), pk_cols.clone());

        let without_rowid = obj
            .sql
            .as_deref()
            .map(|s| s.to_uppercase().contains("WITHOUT ROWID"))
            .unwrap_or(false);
        let mut meta = EngineMeta::new();
        meta.insert("withoutRowid".into(), json!(without_rowid));
        tables.insert(
            obj.name.clone(),
            Table {
                stable_id: StableId::table(E, NS, &obj.name),
                name: obj.name.clone(),
                namespace: NS.into(),
                comment: None,
                estimated_rows: None,
                size_bytes: None,
                columns,
                primary_key: (!pk_cols.is_empty()).then(|| PrimaryKey {
                    stable_id: StableId::child(E, "constraint", NS, &obj.name, "primary"),
                    name: format!("{}_pk", obj.name),
                    namespace: NS.into(),
                    columns: pk_cols,
                    engine_specific_metadata: EngineMeta::new(),
                }),
                foreign_keys: vec![],
                unique_constraints: vec![],
                check_constraints: vec![],
                indexes: vec![],
                engine_specific_metadata: meta,
            },
        );
    }

    for name in &table_names {
        // Foreign keys (grouped by id, ordered by seq => composite FKs keep column pairing)
        let fks = sqlx::query(
            "SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete
             FROM pragma_foreign_key_list(?) ORDER BY id, seq",
        )
        .bind(name)
        .fetch_all(pool)
        .await
        .map_err(map)?;
        type FkGroup = (String, Vec<String>, Vec<Option<String>>, String, String);
        let mut grouped: BTreeMap<i64, FkGroup> = BTreeMap::new();
        for r in &fks {
            let e = grouped.entry(r.get("id")).or_insert_with(|| {
                (
                    r.get("table"),
                    vec![],
                    vec![],
                    r.get("on_update"),
                    r.get("on_delete"),
                )
            });
            e.1.push(r.get("from"));
            e.2.push(r.get("to"));
        }
        for (id, (target, from, to, on_update, on_delete)) in grouped {
            if !tables.contains_key(&target) {
                schema.warnings.push(format!(
                    "foreign key on {name} references missing table {target}"
                ));
                continue;
            }
            let target_cols: Vec<String> = if to.iter().all(Option::is_some) {
                to.into_iter().flatten().collect()
            } else {
                pk_of.get(&target).cloned().unwrap_or_default()
            };
            if target_cols.len() != from.len() {
                schema.warnings.push(format!(
                    "foreign key on {name} -> {target} could not be resolved"
                ));
                continue;
            }
            let fk_name = format!("fk_{name}_{id}");
            if let Some(t) = tables.get_mut(*name) {
                t.foreign_keys.push(ForeignKey {
                    stable_id: StableId::child(E, "constraint", NS, name, &fk_name),
                    name: fk_name,
                    namespace: NS.into(),
                    source_table: StableId::table(E, NS, name),
                    source_columns: from,
                    target_table: StableId::table(E, NS, &target),
                    target_columns: target_cols,
                    on_delete: action(&on_delete),
                    on_update: action(&on_update),
                    engine_specific_metadata: EngineMeta::new(),
                });
            }
        }

        // Indexes
        let idxs = sqlx::query(
            "SELECT name, \"unique\", origin, partial FROM pragma_index_list(?) ORDER BY name",
        )
        .bind(name)
        .fetch_all(pool)
        .await
        .map_err(map)?;
        for ix in &idxs {
            let iname: String = ix.get("name");
            let unique = ix.get::<i64, _>("unique") != 0;
            let origin: String = ix.get("origin");
            let partial = ix.get::<i64, _>("partial") != 0;
            let parts =
                sqlx::query("SELECT name, \"key\" FROM pragma_index_xinfo(?) ORDER BY seqno")
                    .bind(&iname)
                    .fetch_all(pool)
                    .await
                    .map_err(map)?;
            let mut columns = vec![];
            let mut included = vec![];
            let mut has_expr = false;
            for p in &parts {
                let col: Option<String> = p.get("name");
                let is_key = p.get::<i64, _>("key") != 0;
                match (col, is_key) {
                    (Some(c), true) => columns.push(c),
                    (Some(c), false) => {
                        if c != "rowid" {
                            included.push(c)
                        }
                    }
                    (None, true) => has_expr = true,
                    (None, false) => {}
                }
            }
            let sql = objs
                .iter()
                .find(|o| o.kind == "index" && o.name == iname)
                .and_then(|o| o.sql.clone());
            let predicate = if partial {
                sql.as_deref().and_then(|s| {
                    let up = s.to_uppercase();
                    up.rfind(" WHERE ").map(|i| s[i + 7..].trim().to_string())
                })
            } else {
                None
            };
            if origin == "u" {
                if let Some(t) = tables.get_mut(*name) {
                    t.unique_constraints.push(UniqueConstraint {
                        stable_id: StableId::child(E, "constraint", NS, name, &iname),
                        name: iname.clone(),
                        namespace: NS.into(),
                        columns: columns.clone(),
                        engine_specific_metadata: EngineMeta::new(),
                    });
                }
            }
            if origin == "pk" && columns.is_empty() {
                continue;
            }
            let mut meta = EngineMeta::new();
            meta.insert("origin".into(), json!(origin));
            if has_expr {
                meta.insert("hasExpression".into(), json!(true));
            }
            if let Some(t) = tables.get_mut(*name) {
                t.indexes.push(Index {
                    stable_id: StableId::object(E, "index", NS, &iname),
                    name: iname,
                    namespace: NS.into(),
                    table: StableId::table(E, NS, name),
                    columns,
                    unique,
                    primary: origin == "pk",
                    method: Some("btree".into()),
                    predicate,
                    expressions: vec![],
                    included_columns: included,
                    definition: sql,
                    engine_specific_metadata: meta,
                });
            }
        }
    }

    for obj in objs.iter().filter(|o| o.kind == "view") {
        let cols = sqlx::query(
            "SELECT cid, name, type, \"notnull\" FROM pragma_table_info(?) ORDER BY cid",
        )
        .bind(&obj.name)
        .fetch_all(pool)
        .await
        .map_err(map)?;
        schema.views.push(View {
            stable_id: StableId::object(E, "view", NS, &obj.name),
            name: obj.name.clone(),
            namespace: NS.into(),
            materialized: false,
            definition: obj.sql.clone(),
            columns: cols
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let native: String = c.get("type");
                    let name: String = c.get("name");
                    Column {
                        stable_id: StableId::child(E, "column", NS, &obj.name, &name),
                        name,
                        namespace: NS.into(),
                        ordinal: (i + 1) as i32,
                        data_type: if native.is_empty() {
                            "any".into()
                        } else {
                            normalize_type(E, &native)
                        },
                        native_type: native,
                        nullable: true,
                        default: None,
                        generated: None,
                        identity: None,
                        comment: None,
                        engine_specific_metadata: EngineMeta::new(),
                    }
                })
                .collect(),
            comment: None,
            engine_specific_metadata: EngineMeta::new(),
        });
    }

    for obj in objs.iter().filter(|o| o.kind == "trigger") {
        let sql = obj.sql.clone().unwrap_or_default();
        let (timing, events) = parse_trigger_header(&sql);
        let table = StableId::table(E, NS, &obj.table);
        let on_view = schema.views.iter().any(|v| v.name == obj.table);
        if !tables.contains_key(&obj.table) && !on_view {
            continue;
        }
        let table = if on_view {
            StableId::object(E, "view", NS, &obj.table)
        } else {
            table
        };
        schema.triggers.push(Trigger {
            stable_id: StableId::child(E, "trigger", NS, &obj.table, &obj.name),
            name: obj.name.clone(),
            namespace: NS.into(),
            table,
            timing,
            events,
            definition: Some(sql),
            function: None,
            engine_specific_metadata: EngineMeta::new(),
        });
    }

    // Dependencies
    for t in tables.values() {
        for fk in &t.foreign_keys {
            schema.dependencies.push(Dependency {
                from: fk.source_table.clone(),
                to: fk.target_table.clone(),
                kind: DependencyKind::ForeignKey,
            });
        }
    }
    // SQLite has no dependency catalog for views: resolve by scanning the stored SQL for
    // known object names as whole identifiers. This is a heuristic, flagged in metadata.
    for v in &schema.views {
        let def = v.definition.clone().unwrap_or_default();
        for (name, t) in &tables {
            if mentions(&def, name) {
                schema.dependencies.push(Dependency {
                    from: v.stable_id.clone(),
                    to: t.stable_id.clone(),
                    kind: DependencyKind::DependsOn,
                });
            }
        }
    }
    let view_defs: Vec<(StableId, String, String)> = schema
        .views
        .iter()
        .map(|v| {
            (
                v.stable_id.clone(),
                v.name.clone(),
                v.definition.clone().unwrap_or_default(),
            )
        })
        .collect();
    for (id, _, def) in &view_defs {
        for (oid, oname, _) in &view_defs {
            if oid != id && mentions(def, oname) {
                schema.dependencies.push(Dependency {
                    from: id.clone(),
                    to: oid.clone(),
                    kind: DependencyKind::DependsOn,
                });
            }
        }
    }
    for tr in &schema.triggers {
        schema.dependencies.push(Dependency {
            from: tr.stable_id.clone(),
            to: tr.table.clone(),
            kind: DependencyKind::TriggerDependency,
        });
    }
    schema
        .dependencies
        .sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
    schema.dependencies.dedup();

    schema.tables = tables.into_values().collect();
    sort(&mut schema);
    Ok(schema)
}

fn sort(s: &mut DatabaseSchema) {
    s.tables.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    s.views.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    s.triggers.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    for t in &mut s.tables {
        t.foreign_keys.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.unique_constraints
            .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.indexes.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    }
    s.warnings.sort();
}

fn action(s: &str) -> ReferentialAction {
    match s.to_uppercase().as_str() {
        "CASCADE" => ReferentialAction::Cascade,
        "RESTRICT" => ReferentialAction::Restrict,
        "SET NULL" => ReferentialAction::SetNull,
        "SET DEFAULT" => ReferentialAction::SetDefault,
        _ => ReferentialAction::NoAction,
    }
}

fn parse_trigger_header(sql: &str) -> (String, Vec<String>) {
    let up = sql.to_uppercase();
    let head = up.split(" ON ").next().unwrap_or(&up);
    let timing = if head.contains("INSTEAD OF") {
        "INSTEAD OF"
    } else if head.contains("BEFORE") {
        "BEFORE"
    } else {
        "AFTER"
    };
    let events = ["INSERT", "UPDATE", "DELETE"]
        .iter()
        .filter(|e| head.contains(*e))
        .map(|e| e.to_string())
        .collect();
    (timing.into(), events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_header_parsing() {
        let (t, e) = parse_trigger_header(
            "CREATE TRIGGER x BEFORE INSERT OR UPDATE ON users BEGIN SELECT 1; END",
        );
        assert_eq!(t, "BEFORE");
        assert_eq!(e, ["INSERT", "UPDATE"]);
        let (t, _) =
            parse_trigger_header("CREATE TRIGGER x INSTEAD OF DELETE ON v BEGIN SELECT 1; END");
        assert_eq!(t, "INSTEAD OF");
    }
}
