//! MySQL adapter: information_schema for the connected database (MySQL "schema" == database).
//! Text columns are cast to CHAR so sqlx never sees the binary collations information_schema uses.

use std::collections::{BTreeMap, HashMap};

use serde_json::json;
use sqlx::{MySqlPool, Row};

use crate::error::{AppError, AppResult};
use crate::metadata::types::normalize_type;
use crate::metadata::*;

const E: Engine = Engine::Mysql;

fn map(e: sqlx::Error) -> AppError {
    AppError::Introspection(e.to_string())
}

const TABLES: &str = "SELECT CAST(TABLE_NAME AS CHAR) AS name, CAST(TABLE_TYPE AS CHAR) AS kind,
    CAST(TABLE_ROWS AS SIGNED) AS est_rows, CAST(DATA_LENGTH + INDEX_LENGTH AS SIGNED) AS size,
    CAST(TABLE_COMMENT AS CHAR) AS comment, CAST(ENGINE AS CHAR) AS engine
  FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME";

const COLUMNS: &str = "SELECT CAST(TABLE_NAME AS CHAR) AS tbl, CAST(COLUMN_NAME AS CHAR) AS name,
    CAST(ORDINAL_POSITION AS SIGNED) AS ordinal, CAST(COLUMN_TYPE AS CHAR) AS column_type,
    CAST(IS_NULLABLE AS CHAR) AS nullable, CAST(COLUMN_DEFAULT AS CHAR) AS dflt,
    CAST(EXTRA AS CHAR) AS extra, CAST(GENERATION_EXPRESSION AS CHAR) AS gen_expr,
    CAST(COLUMN_COMMENT AS CHAR) AS comment
  FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME, ORDINAL_POSITION";

const CONSTRAINTS: &str = "SELECT CAST(tc.TABLE_NAME AS CHAR) AS tbl, CAST(tc.CONSTRAINT_NAME AS CHAR) AS name,
    CAST(tc.CONSTRAINT_TYPE AS CHAR) AS kind, CAST(kcu.COLUMN_NAME AS CHAR) AS col,
    CAST(kcu.ORDINAL_POSITION AS SIGNED) AS pos, CAST(kcu.REFERENCED_TABLE_SCHEMA AS CHAR) AS ref_schema,
    CAST(kcu.REFERENCED_TABLE_NAME AS CHAR) AS ref_table, CAST(kcu.REFERENCED_COLUMN_NAME AS CHAR) AS ref_col,
    CAST(rc.UPDATE_RULE AS CHAR) AS on_update, CAST(rc.DELETE_RULE AS CHAR) AS on_delete
  FROM information_schema.TABLE_CONSTRAINTS tc
  LEFT JOIN information_schema.KEY_COLUMN_USAGE kcu
    ON kcu.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME
   AND kcu.TABLE_NAME = tc.TABLE_NAME
  LEFT JOIN information_schema.REFERENTIAL_CONSTRAINTS rc
    ON rc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND rc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME
   AND rc.TABLE_NAME = tc.TABLE_NAME
  WHERE tc.TABLE_SCHEMA = DATABASE() AND tc.CONSTRAINT_TYPE IN ('PRIMARY KEY','FOREIGN KEY','UNIQUE')
  ORDER BY tc.TABLE_NAME, tc.CONSTRAINT_NAME, kcu.ORDINAL_POSITION";

const CHECKS: &str =
    "SELECT CAST(tc.TABLE_NAME AS CHAR) AS tbl, CAST(cc.CONSTRAINT_NAME AS CHAR) AS name,
    CAST(cc.CHECK_CLAUSE AS CHAR) AS clause
  FROM information_schema.TABLE_CONSTRAINTS tc
  JOIN information_schema.CHECK_CONSTRAINTS cc
    ON cc.CONSTRAINT_SCHEMA = tc.CONSTRAINT_SCHEMA AND cc.CONSTRAINT_NAME = tc.CONSTRAINT_NAME
  WHERE tc.TABLE_SCHEMA = DATABASE() AND tc.CONSTRAINT_TYPE = 'CHECK' ORDER BY 1, 2";

const INDEXES: &str = "SELECT CAST(TABLE_NAME AS CHAR) AS tbl, CAST(INDEX_NAME AS CHAR) AS name,
    CAST(SEQ_IN_INDEX AS SIGNED) AS seq, CAST(COLUMN_NAME AS CHAR) AS col,
    CAST(EXPRESSION AS CHAR) AS expr, CAST(NON_UNIQUE AS SIGNED) AS non_unique,
    CAST(INDEX_TYPE AS CHAR) AS method
  FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = DATABASE()
  ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX";

const VIEWS: &str = "SELECT CAST(TABLE_NAME AS CHAR) AS name, CAST(VIEW_DEFINITION AS CHAR) AS def
  FROM information_schema.VIEWS WHERE TABLE_SCHEMA = DATABASE() ORDER BY TABLE_NAME";

const VIEW_USAGE: &str =
    "SELECT CAST(VIEW_NAME AS CHAR) AS view_name, CAST(TABLE_SCHEMA AS CHAR) AS src_schema,
    CAST(TABLE_NAME AS CHAR) AS src_name
  FROM information_schema.VIEW_TABLE_USAGE WHERE VIEW_SCHEMA = DATABASE() ORDER BY 1, 2, 3";

const TRIGGERS: &str =
    "SELECT CAST(TRIGGER_NAME AS CHAR) AS name, CAST(EVENT_OBJECT_TABLE AS CHAR) AS tbl,
    CAST(ACTION_TIMING AS CHAR) AS timing, CAST(EVENT_MANIPULATION AS CHAR) AS event,
    CAST(ACTION_STATEMENT AS CHAR) AS body
  FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA = DATABASE() ORDER BY TRIGGER_NAME";

const ROUTINES: &str = "SELECT CAST(r.ROUTINE_NAME AS CHAR) AS name, CAST(r.ROUTINE_TYPE AS CHAR) AS kind,
    CAST(r.DTD_IDENTIFIER AS CHAR) AS returns, CAST(r.ROUTINE_DEFINITION AS CHAR) AS body,
    CAST(r.EXTERNAL_LANGUAGE AS CHAR) AS language,
    CAST((SELECT GROUP_CONCAT(CONCAT(COALESCE(p.PARAMETER_MODE, ''), ' ', p.PARAMETER_NAME, ' ', p.DTD_IDENTIFIER)
                              ORDER BY p.ORDINAL_POSITION SEPARATOR ', ')
          FROM information_schema.PARAMETERS p
          WHERE p.SPECIFIC_SCHEMA = r.ROUTINE_SCHEMA AND p.SPECIFIC_NAME = r.SPECIFIC_NAME
            AND p.ORDINAL_POSITION > 0) AS CHAR) AS args
  FROM information_schema.ROUTINES r WHERE r.ROUTINE_SCHEMA = DATABASE() ORDER BY r.ROUTINE_NAME";

pub async fn introspect(pool: &MySqlPool) -> AppResult<DatabaseSchema> {
    let version: String = sqlx::query_scalar("SELECT CAST(VERSION() AS CHAR)")
        .fetch_one(pool)
        .await
        .map_err(map)?;
    let ns: Option<String> = sqlx::query_scalar("SELECT CAST(DATABASE() AS CHAR)")
        .fetch_one(pool)
        .await
        .map_err(map)?;
    let ns = ns.ok_or_else(|| AppError::Introspection("no database selected".into()))?;

    let mut schema = DatabaseSchema::empty(E, ns.clone());
    schema.server_version = Some(version);
    schema.namespaces.push(DatabaseNamespace {
        stable_id: StableId::namespace(E, &ns),
        name: ns.clone(),
        is_system: false,
        engine_specific_metadata: EngineMeta::new(),
    });

    let tables = sqlx::query(TABLES).fetch_all(pool).await.map_err(map)?;
    let columns = sqlx::query(COLUMNS).fetch_all(pool).await.map_err(map)?;
    let constraints = sqlx::query(CONSTRAINTS)
        .fetch_all(pool)
        .await
        .map_err(map)?;
    let checks = sqlx::query(CHECKS).fetch_all(pool).await.map_err(map)?;
    let indexes = sqlx::query(INDEXES).fetch_all(pool).await.map_err(map)?;
    let views = sqlx::query(VIEWS).fetch_all(pool).await.map_err(map)?;
    let usage = sqlx::query(VIEW_USAGE).fetch_all(pool).await.map_err(map)?;
    let triggers = sqlx::query(TRIGGERS).fetch_all(pool).await.map_err(map)?;
    let routines = sqlx::query(ROUTINES).fetch_all(pool).await.map_err(map)?;

    let mut cols: HashMap<String, Vec<Column>> = HashMap::new();
    for r in &columns {
        let tbl: String = r.get("tbl");
        let name: String = r.get("name");
        let native: String = r.get("column_type");
        let extra: String = r.get::<Option<String>, _>("extra").unwrap_or_default();
        let extra_l = extra.to_lowercase();
        let gen_expr: Option<String> = r.get("gen_expr");
        let generated = if extra_l.contains("virtual generated") {
            Some("virtual".to_string())
        } else if extra_l.contains("stored generated") {
            Some("stored".to_string())
        } else {
            None
        };
        let mut meta = EngineMeta::new();
        if let Some(g) = gen_expr.filter(|g| !g.is_empty()) {
            meta.insert("generationExpression".into(), json!(g));
        }
        cols.entry(tbl.clone()).or_default().push(Column {
            stable_id: StableId::child(E, "column", &ns, &tbl, &name),
            name,
            namespace: ns.clone(),
            ordinal: r.get::<i64, _>("ordinal") as i32,
            data_type: normalize_type(E, &native),
            native_type: native,
            nullable: r.get::<String, _>("nullable") == "YES",
            default: r.get("dflt"),
            generated,
            identity: extra_l
                .contains("auto_increment")
                .then(|| "auto_increment".to_string()),
            comment: r
                .get::<Option<String>, _>("comment")
                .filter(|c| !c.is_empty()),
            engine_specific_metadata: meta,
        });
    }

    let mut by_table: BTreeMap<String, Table> = BTreeMap::new();
    for r in &tables {
        let kind: String = r.get("kind");
        let name: String = r.get("name");
        let c = cols.remove(&name).unwrap_or_default();
        if kind == "VIEW" {
            continue;
        }
        let mut meta = EngineMeta::new();
        if let Some(engine) = r.get::<Option<String>, _>("engine") {
            meta.insert("storageEngine".into(), json!(engine));
        }
        by_table.insert(
            name.clone(),
            Table {
                stable_id: StableId::table(E, &ns, &name),
                name,
                namespace: ns.clone(),
                comment: r
                    .get::<Option<String>, _>("comment")
                    .filter(|c| !c.is_empty()),
                estimated_rows: r.get("est_rows"),
                size_bytes: r.get("size"),
                columns: c,
                primary_key: None,
                foreign_keys: vec![],
                unique_constraints: vec![],
                check_constraints: vec![],
                indexes: vec![],
                engine_specific_metadata: meta,
            },
        );
    }

    // Constraints: rows are one per column, grouped here to keep composite keys intact.
    struct Group {
        kind: String,
        cols: Vec<String>,
        ref_schema: Option<String>,
        ref_table: Option<String>,
        ref_cols: Vec<String>,
        on_update: Option<String>,
        on_delete: Option<String>,
    }
    let mut groups: BTreeMap<(String, String), Group> = BTreeMap::new();
    for r in &constraints {
        let key = (r.get::<String, _>("tbl"), r.get::<String, _>("name"));
        let g = groups.entry(key).or_insert_with(|| Group {
            kind: r.get("kind"),
            cols: vec![],
            ref_schema: r.get("ref_schema"),
            ref_table: r.get("ref_table"),
            ref_cols: vec![],
            on_update: r.get("on_update"),
            on_delete: r.get("on_delete"),
        });
        if let Some(c) = r.get::<Option<String>, _>("col") {
            g.cols.push(c);
        }
        if let Some(c) = r.get::<Option<String>, _>("ref_col") {
            g.ref_cols.push(c);
        }
    }
    for ((tbl, name), g) in groups {
        let Some(t) = by_table.get_mut(&tbl) else {
            continue;
        };
        let sid = StableId::child(E, "constraint", &ns, &tbl, &name);
        match g.kind.as_str() {
            "PRIMARY KEY" => {
                t.primary_key = Some(PrimaryKey {
                    stable_id: sid,
                    name,
                    namespace: ns.clone(),
                    columns: g.cols,
                    engine_specific_metadata: EngineMeta::new(),
                })
            }
            "UNIQUE" => t.unique_constraints.push(UniqueConstraint {
                stable_id: sid,
                name,
                namespace: ns.clone(),
                columns: g.cols,
                engine_specific_metadata: EngineMeta::new(),
            }),
            "FOREIGN KEY" => {
                let (Some(rs), Some(rt)) = (g.ref_schema, g.ref_table) else {
                    continue;
                };
                if rs != ns {
                    schema.warnings.push(format!(
                        "foreign key {tbl}.{name} references {rs}.{rt} in another database (not introspected)"
                    ));
                    continue;
                }
                t.foreign_keys.push(ForeignKey {
                    stable_id: sid,
                    name,
                    namespace: ns.clone(),
                    source_table: StableId::table(E, &ns, &tbl),
                    source_columns: g.cols,
                    target_table: StableId::table(E, &ns, &rt),
                    target_columns: g.ref_cols,
                    on_delete: action(g.on_delete.as_deref()),
                    on_update: action(g.on_update.as_deref()),
                    engine_specific_metadata: EngineMeta::new(),
                });
            }
            _ => {}
        }
    }
    for r in &checks {
        let tbl: String = r.get("tbl");
        let name: String = r.get("name");
        if let Some(t) = by_table.get_mut(&tbl) {
            t.check_constraints.push(CheckConstraint {
                stable_id: StableId::child(E, "constraint", &ns, &tbl, &name),
                name,
                namespace: ns.clone(),
                expression: r.get("clause"),
                engine_specific_metadata: EngineMeta::new(),
            });
        }
    }

    // Indexes
    #[derive(Default)]
    struct Ix {
        columns: Vec<String>,
        expressions: Vec<String>,
        non_unique: bool,
        method: String,
    }
    let mut ixs: BTreeMap<(String, String), Ix> = BTreeMap::new();
    for r in &indexes {
        let ix = ixs.entry((r.get("tbl"), r.get("name"))).or_default();
        ix.non_unique = r.get::<i64, _>("non_unique") != 0;
        ix.method = r.get("method");
        match (
            r.get::<Option<String>, _>("col"),
            r.get::<Option<String>, _>("expr"),
        ) {
            (Some(c), _) => ix.columns.push(c),
            (None, Some(e)) => ix.expressions.push(e),
            _ => {}
        }
    }
    for ((tbl, name), ix) in ixs {
        let Some(t) = by_table.get_mut(&tbl) else {
            continue;
        };
        let primary = name == "PRIMARY";
        t.indexes.push(Index {
            stable_id: StableId::object(E, "index", &ns, &format!("{tbl}.{name}")),
            name,
            namespace: ns.clone(),
            table: StableId::table(E, &ns, &tbl),
            columns: ix.columns,
            unique: !ix.non_unique,
            primary,
            method: Some(ix.method.to_lowercase()),
            predicate: None,
            expressions: ix.expressions,
            included_columns: vec![],
            definition: None,
            engine_specific_metadata: EngineMeta::new(),
        });
    }

    for r in &views {
        let name: String = r.get("name");
        let c = cols.remove(&name).unwrap_or_default();
        schema.views.push(View {
            stable_id: StableId::object(E, "view", &ns, &name),
            name,
            namespace: ns.clone(),
            materialized: false,
            definition: r.get("def"),
            columns: c,
            comment: None,
            engine_specific_metadata: EngineMeta::new(),
        });
    }

    for r in &routines {
        let name: String = r.get("name");
        let args: String = r.get::<Option<String>, _>("args").unwrap_or_default();
        let is_proc = r.get::<String, _>("kind") == "PROCEDURE";
        schema.routines.push(Routine {
            stable_id: StableId::routine(E, &ns, &name, &args),
            name,
            namespace: ns.clone(),
            kind: if is_proc {
                RoutineKind::Procedure
            } else {
                RoutineKind::Function
            },
            language: r.get("language"),
            arguments: args,
            return_type: r.get("returns"),
            definition: r.get("body"),
            engine_specific_metadata: EngineMeta::new(),
        });
    }

    for r in &triggers {
        let name: String = r.get("name");
        let tbl: String = r.get("tbl");
        let event: String = r.get("event");
        let table = StableId::table(E, &ns, &tbl);
        if !by_table.contains_key(&tbl) {
            continue;
        }
        schema.triggers.push(Trigger {
            stable_id: StableId::child(E, "trigger", &ns, &tbl, &name),
            name,
            namespace: ns.clone(),
            table,
            timing: r.get("timing"),
            events: vec![event],
            definition: r.get("body"),
            function: None,
            engine_specific_metadata: EngineMeta::new(),
        });
    }

    // Dependencies
    for t in by_table.values() {
        for fk in &t.foreign_keys {
            schema.dependencies.push(Dependency {
                from: fk.source_table.clone(),
                to: fk.target_table.clone(),
                kind: DependencyKind::ForeignKey,
            });
        }
    }
    for r in &usage {
        let view: String = r.get("view_name");
        let src_schema: String = r.get("src_schema");
        let src: String = r.get("src_name");
        if src_schema != ns {
            continue;
        }
        let to = if by_table.contains_key(&src) {
            StableId::table(E, &ns, &src)
        } else if schema.views.iter().any(|v| v.name == src) {
            StableId::object(E, "view", &ns, &src)
        } else {
            continue;
        };
        schema.dependencies.push(Dependency {
            from: StableId::object(E, "view", &ns, &view),
            to,
            kind: DependencyKind::DependsOn,
        });
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

    schema.tables = by_table.into_values().collect();
    schema.tables.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    schema.views.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    schema
        .triggers
        .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    schema
        .routines
        .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    for t in &mut schema.tables {
        t.foreign_keys.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.unique_constraints
            .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.check_constraints
            .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.indexes.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    }
    schema.warnings.sort();
    Ok(schema)
}

fn action(rule: Option<&str>) -> ReferentialAction {
    match rule.map(str::to_uppercase).as_deref() {
        Some("CASCADE") => ReferentialAction::Cascade,
        Some("RESTRICT") => ReferentialAction::Restrict,
        Some("SET NULL") => ReferentialAction::SetNull,
        Some("SET DEFAULT") => ReferentialAction::SetDefault,
        _ => ReferentialAction::NoAction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referential_rules() {
        assert_eq!(action(Some("CASCADE")), ReferentialAction::Cascade);
        assert_eq!(action(Some("SET NULL")), ReferentialAction::SetNull);
        assert_eq!(action(Some("NO ACTION")), ReferentialAction::NoAction);
        assert_eq!(action(None), ReferentialAction::NoAction);
    }
}
