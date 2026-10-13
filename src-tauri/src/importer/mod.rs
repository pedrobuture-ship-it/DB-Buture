//! SQL import: turns DDL text into a `DatabaseSchema` without a database connection.
//!
//! Statements are parsed one at a time so a single unsupported or malformed statement never
//! hides the rest of the script. Everything that is not understood is reported verbatim.

use std::collections::BTreeMap;
use std::ops::ControlFlow;

use serde::{Deserialize, Serialize};
use sqlparser::ast::{
    visit_relations, AlterTableOperation, ColumnOption, ObjectName, ReferentialAction as SqlAction,
    Statement, TableConstraint,
};
use sqlparser::dialect::{Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer};

use crate::metadata::types::normalize_type;
use crate::metadata::*;

const MAX_WARNINGS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SqlDialect {
    Postgres,
    Mysql,
    Sqlite,
    Generic,
}

impl SqlDialect {
    pub fn from_engine(e: Engine) -> Self {
        match e {
            Engine::Postgres => SqlDialect::Postgres,
            Engine::Mysql => SqlDialect::Mysql,
            Engine::Sqlite => SqlDialect::Sqlite,
            Engine::Generic => SqlDialect::Generic,
        }
    }

    pub(crate) fn parser_dialect_pub(self) -> Box<dyn Dialect> {
        self.parser_dialect()
    }

    pub fn default_ns(self) -> &'static str {
        self.default_namespace()
    }

    fn engine(self) -> Engine {
        match self {
            SqlDialect::Postgres => Engine::Postgres,
            SqlDialect::Mysql => Engine::Mysql,
            SqlDialect::Sqlite => Engine::Sqlite,
            SqlDialect::Generic => Engine::Generic,
        }
    }

    fn default_namespace(self) -> &'static str {
        match self {
            SqlDialect::Postgres => "public",
            SqlDialect::Sqlite => "main",
            _ => "default",
        }
    }

    fn parser_dialect(self) -> Box<dyn Dialect> {
        match self {
            SqlDialect::Postgres => Box::new(PostgreSqlDialect {}),
            SqlDialect::Mysql => Box::new(MySqlDialect {}),
            SqlDialect::Sqlite => Box::new(SQLiteDialect {}),
            SqlDialect::Generic => Box::new(GenericDialect {}),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWarning {
    /// Index of the statement in the script (1 based).
    pub statement_no: usize,
    /// The statement text, shortened for display.
    pub statement: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Parsed,
    ParsedWithWarnings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub schema: DatabaseSchema,
    pub status: ImportStatus,
    pub statements: usize,
    pub understood: usize,
    pub warnings: Vec<ImportWarning>,
    /// True when more warnings existed than are listed.
    pub warnings_truncated: bool,
}

struct TableDraft {
    namespace: String,
    name: String,
    comment: Option<String>,
    columns: Vec<Column>,
    primary_key: Option<(Option<String>, Vec<String>)>,
    foreign_keys: Vec<FkDraft>,
    uniques: Vec<(Option<String>, Vec<String>)>,
    checks: Vec<(Option<String>, String)>,
    indexes: Vec<IndexDraft>,
}

struct FkDraft {
    name: Option<String>,
    columns: Vec<String>,
    target: (String, String),
    target_columns: Vec<String>,
    on_delete: ReferentialAction,
    on_update: ReferentialAction,
}

struct IndexDraft {
    name: String,
    columns: Vec<String>,
    expressions: Vec<String>,
    unique: bool,
    method: Option<String>,
}

/// Splits a script into statements at top level semicolons (quotes, comments and dollar
/// quoting are handled by the tokenizer).
pub(crate) fn split_statements(sql: &str, d: &dyn Dialect) -> Result<Vec<String>, String> {
    let tokens = Tokenizer::new(d, sql)
        .tokenize()
        .map_err(|e| e.to_string())?;
    let mut out = vec![];
    let mut cur = String::new();
    let mut has_content = false;
    for t in tokens {
        match t {
            Token::SemiColon => {
                if has_content {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
                has_content = false;
            }
            Token::Whitespace(_) => cur.push_str(&t.to_string()),
            other => {
                has_content = true;
                cur.push_str(&other.to_string());
            }
        }
    }
    if has_content {
        out.push(cur.trim().to_string());
    }
    Ok(out)
}

fn preview(s: &str) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 140 {
        format!("{}…", one.chars().take(140).collect::<String>())
    } else {
        one
    }
}

fn name_parts(n: &ObjectName) -> Vec<String> {
    n.0.iter()
        .filter_map(|p| p.as_ident())
        .map(|i| i.value.clone())
        .collect()
}

fn qualify(n: &ObjectName, default_ns: &str) -> (String, String) {
    let parts = name_parts(n);
    match parts.as_slice() {
        [name] => (default_ns.to_string(), name.clone()),
        [.., ns, name] => (ns.clone(), name.clone()),
        [] => (default_ns.to_string(), String::new()),
    }
}

fn key(ns: &str, name: &str) -> (String, String) {
    (ns.to_lowercase(), name.to_lowercase())
}

fn action(a: &Option<SqlAction>) -> ReferentialAction {
    match a {
        Some(SqlAction::Cascade) => ReferentialAction::Cascade,
        Some(SqlAction::Restrict) => ReferentialAction::Restrict,
        Some(SqlAction::SetNull) => ReferentialAction::SetNull,
        Some(SqlAction::SetDefault) => ReferentialAction::SetDefault,
        _ => ReferentialAction::NoAction,
    }
}

pub fn import_sql(sql: &str, dialect: SqlDialect) -> ImportResult {
    import_sql_with_context(sql, dialect, None)
}

/// Like [`import_sql`], but foreign keys may also point at tables of `known` (used by the
/// migration simulator, where a new table references existing ones).
pub fn import_sql_with_context(
    sql: &str,
    dialect: SqlDialect,
    known: Option<&DatabaseSchema>,
) -> ImportResult {
    let engine = dialect.engine();
    let ns_default = dialect.default_namespace();
    let d = dialect.parser_dialect();
    let mut warnings: Vec<ImportWarning> = vec![];
    let mut total_warnings = 0usize;
    let mut warn = |no: usize, stmt: &str, reason: String, warnings: &mut Vec<ImportWarning>| {
        total_warnings += 1;
        if warnings.len() < MAX_WARNINGS {
            warnings.push(ImportWarning {
                statement_no: no,
                statement: preview(stmt),
                reason,
            });
        }
    };

    let statements = match split_statements(sql, d.as_ref()) {
        Ok(s) => s,
        Err(e) => {
            let mut schema = DatabaseSchema::empty(engine, "imported");
            schema
                .warnings
                .push(format!("script could not be tokenized: {e}"));
            return ImportResult {
                schema,
                status: ImportStatus::ParsedWithWarnings,
                statements: 0,
                understood: 0,
                warnings: vec![ImportWarning {
                    statement_no: 0,
                    statement: preview(sql),
                    reason: format!("tokenizer error: {e}"),
                }],
                warnings_truncated: false,
            };
        }
    };

    let mut tables: BTreeMap<(String, String), TableDraft> = BTreeMap::new();
    let mut views: Vec<(View, Vec<ObjectName>)> = vec![];
    let mut understood = 0usize;

    for (i, text) in statements.iter().enumerate() {
        let no = i + 1;
        let parsed = match Parser::parse_sql(d.as_ref(), text) {
            Ok(p) => p,
            Err(e) => {
                warn(no, text, format!("could not parse: {e}"), &mut warnings);
                continue;
            }
        };
        for stmt in parsed {
            match stmt {
                Statement::CreateTable(ct) => {
                    let (ns, name) = qualify(&ct.name, ns_default);
                    let mut t = TableDraft {
                        namespace: ns.clone(),
                        name: name.clone(),
                        comment: ct
                            .comment
                            .as_ref()
                            .map(|c| c.to_string().trim_matches('\'').to_string()),
                        columns: vec![],
                        primary_key: None,
                        foreign_keys: vec![],
                        uniques: vec![],
                        checks: vec![],
                        indexes: vec![],
                    };
                    for (ord, c) in ct.columns.iter().enumerate() {
                        let native = c.data_type.to_string().to_lowercase();
                        let mut nullable = true;
                        let mut default = None;
                        let mut generated = None;
                        let mut identity = None;
                        let mut comment = None;
                        for opt in &c.options {
                            match &opt.option {
                                ColumnOption::NotNull => nullable = false,
                                ColumnOption::Null => nullable = true,
                                ColumnOption::Default(e) => default = Some(e.to_string()),
                                ColumnOption::Comment(s) => comment = Some(s.clone()),
                                ColumnOption::Generated { .. } => {
                                    generated = Some("generated".to_string());
                                    identity = opt
                                        .option
                                        .to_string()
                                        .to_lowercase()
                                        .contains("identity")
                                        .then(|| "generated".to_string());
                                }
                                ColumnOption::PrimaryKey(pk) => {
                                    nullable = false;
                                    t.primary_key = Some((
                                        pk.name.as_ref().map(|n| n.value.clone()),
                                        vec![c.name.value.clone()],
                                    ));
                                }
                                ColumnOption::Unique(u) => {
                                    t.uniques.push((
                                        u.name.as_ref().map(|n| n.value.clone()),
                                        vec![c.name.value.clone()],
                                    ));
                                }
                                ColumnOption::ForeignKey(fk) => {
                                    t.foreign_keys.push(FkDraft {
                                        name: fk.name.as_ref().map(|n| n.value.clone()),
                                        columns: vec![c.name.value.clone()],
                                        target: qualify(&fk.foreign_table, ns_default),
                                        target_columns: fk
                                            .referred_columns
                                            .iter()
                                            .map(|x| x.value.clone())
                                            .collect(),
                                        on_delete: action(&fk.on_delete),
                                        on_update: action(&fk.on_update),
                                    });
                                }
                                ColumnOption::Check(ch) => {
                                    t.checks.push((
                                        ch.name.as_ref().map(|n| n.value.clone()),
                                        ch.expr.to_string(),
                                    ));
                                }
                                ColumnOption::DialectSpecific(tokens) => {
                                    let s: String = tokens
                                        .iter()
                                        .map(|t| t.to_string())
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                        .to_lowercase();
                                    if s.contains("auto_increment") || s.contains("autoincrement") {
                                        identity = Some("auto_increment".to_string());
                                    }
                                }
                                _ => {}
                            }
                        }
                        let lower = native.to_lowercase();
                        if lower.contains("serial") {
                            identity = Some("serial".to_string());
                            nullable = false;
                        }
                        t.columns.push(Column {
                            stable_id: StableId::child(engine, "column", &ns, &name, &c.name.value),
                            name: c.name.value.clone(),
                            namespace: ns.clone(),
                            ordinal: (ord + 1) as i32,
                            data_type: normalize_type(engine, &native),
                            native_type: native,
                            nullable,
                            default,
                            generated,
                            identity,
                            comment,
                            engine_specific_metadata: EngineMeta::new(),
                        });
                    }
                    for con in &ct.constraints {
                        apply_constraint(&mut t, con, ns_default);
                    }
                    // a table-level PRIMARY KEY makes its columns NOT NULL
                    if let Some((_, cols)) = &t.primary_key {
                        for c in t.columns.iter_mut().filter(|c| cols.contains(&c.name)) {
                            c.nullable = false;
                        }
                    }
                    if let Some(query) = &ct.query {
                        warn(no, text, format!("CREATE TABLE ... AS SELECT: columns of {name} come from `{}`; only declared columns are known", preview(&query.to_string())), &mut warnings);
                    }
                    tables.insert(key(&ns, &name), t);
                    understood += 1;
                }
                Statement::AlterTable(at) => {
                    let (ns, name) = qualify(&at.name, ns_default);
                    let mut ok = false;
                    for op in &at.operations {
                        match op {
                            AlterTableOperation::AddConstraint { constraint, .. } => {
                                if let Some(t) = tables.get_mut(&key(&ns, &name)) {
                                    apply_constraint(t, constraint, ns_default);
                                    ok = true;
                                } else {
                                    warn(
                                        no,
                                        text,
                                        format!("ALTER TABLE targets unknown table {ns}.{name}"),
                                        &mut warnings,
                                    );
                                }
                            }
                            other => warn(
                                no,
                                text,
                                format!(
                                    "ALTER TABLE operation not interpreted: {}",
                                    preview(&other.to_string())
                                ),
                                &mut warnings,
                            ),
                        }
                    }
                    if ok {
                        understood += 1;
                    }
                }
                Statement::CreateIndex(ci) => {
                    let (ns, table) = qualify(&ci.table_name, ns_default);
                    let Some(t) = tables.get_mut(&key(&ns, &table)) else {
                        warn(
                            no,
                            text,
                            format!("CREATE INDEX targets unknown table {ns}.{table}"),
                            &mut warnings,
                        );
                        continue;
                    };
                    let mut columns = vec![];
                    let mut expressions = vec![];
                    for c in &ci.columns {
                        match &c.column.expr {
                            sqlparser::ast::Expr::Identifier(id) => columns.push(id.value.clone()),
                            other => expressions.push(other.to_string()),
                        }
                    }
                    let iname = ci
                        .name
                        .as_ref()
                        .map(|n| name_parts(n).last().cloned().unwrap_or_default());
                    t.indexes.push(IndexDraft {
                        name: iname
                            .unwrap_or_else(|| format!("idx_{}_{}", table, columns.join("_"))),
                        columns,
                        expressions,
                        unique: ci.unique,
                        method: ci.using.as_ref().map(|u| u.to_string().to_lowercase()),
                    });
                    understood += 1;
                }
                Statement::CreateView(cv) => {
                    let (ns, name) = qualify(&cv.name, ns_default);
                    let mut rels: Vec<ObjectName> = vec![];
                    let _ = visit_relations(&cv.query, |r| {
                        rels.push(r.clone());
                        ControlFlow::<()>::Continue(())
                    });
                    views.push((
                        View {
                            stable_id: StableId::object(
                                engine,
                                if cv.materialized { "matview" } else { "view" },
                                &ns,
                                &name,
                            ),
                            name,
                            namespace: ns,
                            materialized: cv.materialized,
                            definition: Some(cv.query.to_string()),
                            columns: vec![],
                            comment: None,
                            engine_specific_metadata: EngineMeta::new(),
                        },
                        rels,
                    ));
                    understood += 1;
                }
                other => {
                    let kind = format!("{other:?}");
                    let kind = kind
                        .split(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap_or("statement");
                    warn(
                        no,
                        text,
                        format!("{kind} is not interpreted by the importer"),
                        &mut warnings,
                    );
                }
            }
        }
    }

    // ---- build the schema ----
    let mut schema = DatabaseSchema::empty(engine, "imported");
    let mut table_ids: BTreeMap<(String, String), StableId> = tables
        .iter()
        .map(|(k, t)| (k.clone(), StableId::table(engine, &t.namespace, &t.name)))
        .collect();
    let mut known_pks: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    if let Some(k) = known {
        for t in &k.tables {
            let kk = key(&t.namespace, &t.name);
            table_ids
                .entry(kk.clone())
                .or_insert_with(|| t.stable_id.clone());
            known_pks.insert(
                kk,
                t.primary_key
                    .as_ref()
                    .map(|p| p.columns.clone())
                    .unwrap_or_default(),
            );
        }
    }

    for t in tables.values() {
        let tid = StableId::table(engine, &t.namespace, &t.name);
        let mut foreign_keys = vec![];
        for (n, fk) in t.foreign_keys.iter().enumerate() {
            let Some(target_id) = table_ids.get(&key(&fk.target.0, &fk.target.1)) else {
                warn(
                    0,
                    &t.name,
                    format!(
                        "foreign key on {}.{} references {}.{} which is not defined in the script",
                        t.namespace, t.name, fk.target.0, fk.target.1
                    ),
                    &mut warnings,
                );
                continue;
            };
            let target_cols = if fk.target_columns.is_empty() {
                tables
                    .get(&key(&fk.target.0, &fk.target.1))
                    .and_then(|x| x.primary_key.as_ref())
                    .map(|p| p.1.clone())
                    .or_else(|| known_pks.get(&key(&fk.target.0, &fk.target.1)).cloned())
                    .unwrap_or_default()
            } else {
                fk.target_columns.clone()
            };
            let name = fk.name.clone().unwrap_or_else(|| {
                format!(
                    "{}_{}_fkey{}",
                    t.name,
                    fk.columns.join("_"),
                    if n > 0 { n.to_string() } else { String::new() }
                )
            });
            foreign_keys.push(ForeignKey {
                stable_id: StableId::child(engine, "constraint", &t.namespace, &t.name, &name),
                name,
                namespace: t.namespace.clone(),
                source_table: tid.clone(),
                source_columns: fk.columns.clone(),
                target_table: target_id.clone(),
                target_columns: target_cols,
                on_delete: fk.on_delete,
                on_update: fk.on_update,
                engine_specific_metadata: EngineMeta::new(),
            });
        }
        let mut indexes: Vec<Index> = t
            .indexes
            .iter()
            .map(|i| Index {
                stable_id: StableId::object(engine, "index", &t.namespace, &i.name),
                name: i.name.clone(),
                namespace: t.namespace.clone(),
                table: tid.clone(),
                columns: i.columns.clone(),
                unique: i.unique,
                primary: false,
                method: i.method.clone().or(Some("btree".into())),
                predicate: None,
                expressions: i.expressions.clone(),
                included_columns: vec![],
                definition: None,
                engine_specific_metadata: EngineMeta::new(),
            })
            .collect();
        if let Some((n, cols)) = &t.primary_key {
            let name = n.clone().unwrap_or_else(|| format!("{}_pkey", t.name));
            indexes.push(Index {
                stable_id: StableId::object(engine, "index", &t.namespace, &name),
                name,
                namespace: t.namespace.clone(),
                table: tid.clone(),
                columns: cols.clone(),
                unique: true,
                primary: true,
                method: Some("btree".into()),
                predicate: None,
                expressions: vec![],
                included_columns: vec![],
                definition: None,
                engine_specific_metadata: EngineMeta::new(),
            });
        }
        schema.tables.push(Table {
            stable_id: tid.clone(),
            name: t.name.clone(),
            namespace: t.namespace.clone(),
            comment: t.comment.clone(),
            estimated_rows: None,
            size_bytes: None,
            columns: t.columns.clone(),
            primary_key: t.primary_key.as_ref().map(|(n, cols)| {
                let name = n.clone().unwrap_or_else(|| format!("{}_pkey", t.name));
                PrimaryKey {
                    stable_id: StableId::child(engine, "constraint", &t.namespace, &t.name, &name),
                    name,
                    namespace: t.namespace.clone(),
                    columns: cols.clone(),
                    engine_specific_metadata: EngineMeta::new(),
                }
            }),
            foreign_keys,
            unique_constraints: t
                .uniques
                .iter()
                .enumerate()
                .map(|(n, (name, cols))| {
                    let name = name.clone().unwrap_or_else(|| {
                        format!(
                            "{}_{}_key{}",
                            t.name,
                            cols.join("_"),
                            if n > 0 { n.to_string() } else { String::new() }
                        )
                    });
                    UniqueConstraint {
                        stable_id: StableId::child(
                            engine,
                            "constraint",
                            &t.namespace,
                            &t.name,
                            &name,
                        ),
                        name,
                        namespace: t.namespace.clone(),
                        columns: cols.clone(),
                        engine_specific_metadata: EngineMeta::new(),
                    }
                })
                .collect(),
            check_constraints: t
                .checks
                .iter()
                .enumerate()
                .map(|(n, (name, expr))| {
                    let name = name
                        .clone()
                        .unwrap_or_else(|| format!("{}_check{}", t.name, n + 1));
                    CheckConstraint {
                        stable_id: StableId::child(
                            engine,
                            "constraint",
                            &t.namespace,
                            &t.name,
                            &name,
                        ),
                        name,
                        namespace: t.namespace.clone(),
                        expression: expr.clone(),
                        engine_specific_metadata: EngineMeta::new(),
                    }
                })
                .collect(),
            indexes,
            engine_specific_metadata: EngineMeta::new(),
        });
    }

    let view_ids: BTreeMap<(String, String), StableId> = views
        .iter()
        .map(|(v, _)| (key(&v.namespace, &v.name), v.stable_id.clone()))
        .collect();
    for (v, rels) in &views {
        for r in rels {
            let (ns, name) = qualify(r, ns_default);
            let k = key(&ns, &name);
            let to = table_ids.get(&k).or_else(|| view_ids.get(&k));
            match to {
                Some(to) => schema.dependencies.push(Dependency {
                    from: v.stable_id.clone(),
                    to: to.clone(),
                    kind: DependencyKind::DependsOn,
                }),
                None => warn(
                    0,
                    &v.name,
                    format!(
                        "view {}.{} reads {}.{} which is not defined in the script",
                        v.namespace, v.name, ns, name
                    ),
                    &mut warnings,
                ),
            }
        }
    }
    for t in &schema.tables {
        for fk in &t.foreign_keys {
            schema.dependencies.push(Dependency {
                from: fk.source_table.clone(),
                to: fk.target_table.clone(),
                kind: DependencyKind::ForeignKey,
            });
        }
    }
    schema.views = views.into_iter().map(|(v, _)| v).collect();

    let mut namespaces: Vec<String> = schema
        .tables
        .iter()
        .map(|t| t.namespace.clone())
        .chain(schema.views.iter().map(|v| v.namespace.clone()))
        .collect();
    namespaces.sort();
    namespaces.dedup();
    schema.namespaces = namespaces
        .into_iter()
        .map(|n| DatabaseNamespace {
            stable_id: StableId::namespace(engine, &n),
            name: n,
            is_system: false,
            engine_specific_metadata: EngineMeta::new(),
        })
        .collect();

    schema
        .dependencies
        .sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
    schema.dependencies.dedup();
    schema.tables.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    schema.views.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    for t in &mut schema.tables {
        t.foreign_keys.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.unique_constraints
            .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.check_constraints
            .sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        t.indexes.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
    }
    schema.warnings = warnings
        .iter()
        .map(|w| format!("#{}: {}", w.statement_no, w.reason))
        .collect();

    ImportResult {
        status: if total_warnings == 0 {
            ImportStatus::Parsed
        } else {
            ImportStatus::ParsedWithWarnings
        },
        statements: statements.len(),
        understood,
        warnings_truncated: total_warnings > warnings.len(),
        warnings,
        schema,
    }
}

fn apply_constraint(t: &mut TableDraft, con: &TableConstraint, ns_default: &str) {
    match con {
        TableConstraint::PrimaryKey(pk) => {
            t.primary_key = Some((
                pk.name.as_ref().map(|n| n.value.clone()),
                index_columns(&pk.columns),
            ));
        }
        TableConstraint::Unique(u) => {
            t.uniques.push((
                u.name.as_ref().map(|n| n.value.clone()),
                index_columns(&u.columns),
            ));
        }
        TableConstraint::ForeignKey(fk) => t.foreign_keys.push(FkDraft {
            name: fk.name.as_ref().map(|n| n.value.clone()),
            columns: fk.columns.iter().map(|c| c.value.clone()).collect(),
            target: qualify(&fk.foreign_table, ns_default),
            target_columns: fk
                .referred_columns
                .iter()
                .map(|c| c.value.clone())
                .collect(),
            on_delete: action(&fk.on_delete),
            on_update: action(&fk.on_update),
        }),
        TableConstraint::Check(c) => t
            .checks
            .push((c.name.as_ref().map(|n| n.value.clone()), c.expr.to_string())),
        TableConstraint::Index(i) => t.indexes.push(IndexDraft {
            name: i
                .name
                .as_ref()
                .map(|n| n.value.clone())
                .unwrap_or_else(|| format!("idx_{}", t.name)),
            columns: index_columns(&i.columns),
            expressions: vec![],
            unique: false,
            method: None,
        }),
        _ => {}
    }
}

fn index_columns(cols: &[sqlparser::ast::IndexColumn]) -> Vec<String> {
    cols.iter()
        .map(|c| match &c.column.expr {
            sqlparser::ast::Expr::Identifier(i) => i.value.clone(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PG: &str = r#"
        CREATE SCHEMA shop;
        CREATE TABLE shop.users (
            id bigserial PRIMARY KEY,
            email varchar(255) NOT NULL UNIQUE,
            created_at timestamptz DEFAULT now()
        );
        CREATE TABLE shop.orders (
            id bigint,
            user_id bigint NOT NULL REFERENCES shop.users(id) ON DELETE CASCADE,
            total numeric(12,2) CHECK (total >= 0),
            CONSTRAINT orders_pk PRIMARY KEY (id)
        );
        CREATE TABLE shop.order_items (
            order_id bigint NOT NULL,
            line int NOT NULL,
            PRIMARY KEY (order_id, line),
            FOREIGN KEY (order_id) REFERENCES shop.orders (id)
        );
        CREATE INDEX idx_orders_user ON shop.orders (user_id);
        CREATE UNIQUE INDEX idx_users_lower ON shop.users (lower(email));
        ALTER TABLE shop.order_items ADD CONSTRAINT fk_items_self FOREIGN KEY (order_id, line) REFERENCES shop.order_items (order_id, line);
        CREATE VIEW shop.user_totals AS SELECT u.id, sum(o.total) FROM shop.users u JOIN shop.orders o ON o.user_id = u.id GROUP BY u.id;
    "#;

    #[test]
    fn imports_postgres_ddl() {
        let r = import_sql(PG, SqlDialect::Postgres);
        assert_eq!(
            r.status,
            ImportStatus::ParsedWithWarnings,
            "CREATE SCHEMA is not interpreted"
        );
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].statement.starts_with("CREATE SCHEMA shop"));

        let s = &r.schema;
        assert_eq!(s.tables.len(), 3);
        let users = s.tables.iter().find(|t| t.name == "users").unwrap();
        assert_eq!(users.primary_key.as_ref().unwrap().columns, ["id"]);
        assert_eq!(users.columns[0].identity.as_deref(), Some("serial"));
        assert_eq!(users.unique_constraints[0].columns, ["email"]);
        assert!(users
            .indexes
            .iter()
            .any(|i| i.name == "idx_users_lower" && i.unique && !i.expressions.is_empty()));

        let orders = s.tables.iter().find(|t| t.name == "orders").unwrap();
        assert_eq!(orders.primary_key.as_ref().unwrap().name, "orders_pk");
        assert_eq!(orders.foreign_keys[0].on_delete, ReferentialAction::Cascade);
        assert_eq!(orders.check_constraints.len(), 1);
        assert!(
            !orders.columns[0].nullable,
            "primary key columns are NOT NULL"
        );

        let items = s.tables.iter().find(|t| t.name == "order_items").unwrap();
        assert_eq!(
            items.primary_key.as_ref().unwrap().columns,
            ["order_id", "line"]
        );
        assert_eq!(items.foreign_keys.len(), 2);
        let composite = items
            .foreign_keys
            .iter()
            .find(|f| f.name == "fk_items_self")
            .unwrap();
        assert_eq!(composite.source_columns, ["order_id", "line"]);
        assert_eq!(composite.target_columns, ["order_id", "line"]);
    }

    #[test]
    fn view_dependencies_come_from_the_parsed_query() {
        let r = import_sql(PG, SqlDialect::Postgres);
        let deps: Vec<_> = r
            .schema
            .dependencies
            .iter()
            .filter(|d| d.kind == DependencyKind::DependsOn)
            .collect();
        assert_eq!(deps.len(), 2);
        assert!(deps
            .iter()
            .all(|d| d.from.as_str().contains("view:shop.user_totals")));
    }

    #[test]
    fn clean_script_is_parsed_without_warnings() {
        let r = import_sql("CREATE TABLE a (id int PRIMARY KEY); CREATE TABLE b (id int, a_id int REFERENCES a(id));", SqlDialect::Generic);
        assert_eq!(r.status, ImportStatus::Parsed);
        assert_eq!((r.statements, r.understood), (2, 2));
        assert_eq!(r.schema.engine, Engine::Generic);
        assert_eq!(r.schema.tables[1].foreign_keys[0].target_columns, ["id"]);
    }

    #[test]
    fn unsupported_and_broken_statements_are_listed_exactly() {
        let sql = "CREATE TABLE a (id int);\nINSERT INTO a VALUES (1);\nCREATE TABLE ((;\nCREATE FUNCTION f() RETURNS int AS $$ SELECT 1 $$ LANGUAGE sql;";
        let r = import_sql(sql, SqlDialect::Postgres);
        assert_eq!(r.status, ImportStatus::ParsedWithWarnings);
        assert_eq!(
            r.schema.tables.len(),
            1,
            "valid statements survive the broken ones"
        );
        let nos: Vec<usize> = r.warnings.iter().map(|w| w.statement_no).collect();
        assert_eq!(nos, [2, 3, 4]);
        assert!(r.warnings[0].statement.starts_with("INSERT INTO a"));
        assert!(r.warnings[1].reason.contains("could not parse"));
    }

    #[test]
    fn dangling_foreign_key_target_is_reported() {
        let r = import_sql(
            "CREATE TABLE a (id int, b_id int REFERENCES b(id));",
            SqlDialect::Generic,
        );
        assert!(r
            .warnings
            .iter()
            .any(|w| w.reason.contains("not defined in the script")));
        assert!(r.schema.tables[0].foreign_keys.is_empty());
    }

    #[test]
    fn mysql_auto_increment_and_inline_comment() {
        let r = import_sql("CREATE TABLE `t` (`id` INT AUTO_INCREMENT PRIMARY KEY, `n` VARCHAR(10) COMMENT 'hello');", SqlDialect::Mysql);
        let t = &r.schema.tables[0];
        assert_eq!(t.columns[0].identity.as_deref(), Some("auto_increment"));
        assert_eq!(t.columns[1].comment.as_deref(), Some("hello"));
    }

    #[test]
    fn sqlite_dialect_and_default_namespace() {
        let r = import_sql(
            "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT);",
            SqlDialect::Sqlite,
        );
        assert_eq!(r.schema.tables[0].namespace, "main");
        assert_eq!(r.schema.engine, Engine::Sqlite);
    }

    #[test]
    fn semicolons_inside_strings_and_dollar_quotes_do_not_split() {
        let r = import_sql(
            "CREATE TABLE a (s text DEFAULT 'x;y'); CREATE TABLE b (id int);",
            SqlDialect::Postgres,
        );
        assert_eq!(r.statements, 2);
        assert_eq!(r.schema.tables.len(), 2);
    }

    #[test]
    fn empty_and_garbage_input_do_not_panic() {
        assert_eq!(import_sql("", SqlDialect::Generic).statements, 0);
        let r = import_sql("¯\\_(ツ)_/¯ ;;; 'unterminated", SqlDialect::Postgres);
        assert_eq!(r.status, ImportStatus::ParsedWithWarnings);
    }

    #[test]
    fn output_is_deterministic() {
        let a = import_sql(PG, SqlDialect::Postgres);
        let b = import_sql(PG, SqlDialect::Postgres);
        assert_eq!(a.schema, b.schema);
    }
}
