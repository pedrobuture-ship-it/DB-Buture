//! Data lineage for views, combining native catalog dependencies with the parsed definition.
//!
//! Nothing is inferred without evidence: ambiguous or unresolvable references are reported as
//! `UNKNOWN`, and a lineage is only `complete` when parser and catalog agree.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use serde::Serialize;
use sqlparser::ast::{
    visit_expressions, visit_relations, Expr, FunctionArguments, ObjectName, Query, Select,
    SelectItem, SelectItemQualifiedWildcardKind, SetExpr, Statement, TableFactor,
};
use sqlparser::dialect::{Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect};
use sqlparser::parser::Parser;

use crate::metadata::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Parser and catalog agree on every source.
    Complete,
    /// Some information is missing or the two sources disagree.
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Catalog,
    Parser,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSource {
    pub id: StableId,
    pub name: String,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableLineage {
    pub view: StableId,
    pub view_name: String,
    pub sources: Vec<TableSource>,
    pub confidence: Confidence,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ColumnClass {
    Direct,
    Derived,
    Aggregated,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSource {
    pub table: StableId,
    pub table_name: String,
    pub column: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnLineage {
    pub column: String,
    pub class: ColumnClass,
    pub sources: Vec<ColumnSource>,
    pub expression: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewColumnLineage {
    pub view: StableId,
    pub view_name: String,
    pub columns: Vec<ColumnLineage>,
    pub confidence: Confidence,
    pub notes: Vec<String>,
}

const AGGREGATES: &[&str] = &[
    "count",
    "sum",
    "avg",
    "min",
    "max",
    "array_agg",
    "string_agg",
    "json_agg",
    "jsonb_agg",
    "group_concat",
    "bool_and",
    "bool_or",
    "every",
    "stddev",
    "variance",
    "json_arrayagg",
    "json_objectagg",
    "json_object_agg",
    "jsonb_object_agg",
    "total",
];

fn dialect(e: Engine) -> Box<dyn Dialect> {
    match e {
        Engine::Postgres => Box::new(PostgreSqlDialect {}),
        Engine::Mysql => Box::new(MySqlDialect {}),
        Engine::Sqlite => Box::new(SQLiteDialect {}),
        Engine::Generic => Box::new(GenericDialect {}),
    }
}

/// Parses a stored view definition (a bare query, or `CREATE VIEW ... AS <query>` for SQLite).
fn parse_view_query(def: &str, engine: Engine) -> Result<Query, String> {
    let d = dialect(engine);
    let stmts = Parser::parse_sql(d.as_ref(), def).map_err(|e| e.to_string())?;
    match stmts.into_iter().next() {
        Some(Statement::Query(q)) => Ok(*q),
        Some(Statement::CreateView(v)) => Ok(*v.query),
        _ => Err("definition is not a query".into()),
    }
}

/// What the lineage code can resolve a relation name to.
struct Catalog<'a> {
    schema: &'a DatabaseSchema,
}

struct Relation {
    id: StableId,
    name: String,
    columns: Vec<String>,
}

impl<'a> Catalog<'a> {
    fn resolve(&self, name: &ObjectName, view_ns: &str) -> Option<Relation> {
        let parts: Vec<String> = name
            .0
            .iter()
            .filter_map(|p| p.as_ident())
            .map(|i| i.value.clone())
            .collect();
        let (ns, rel) = match parts.as_slice() {
            [n] => (None, n.as_str()),
            [.., ns, n] => (Some(ns.as_str()), n.as_str()),
            [] => return None,
        };
        let matches_ns = |candidate: &str| ns.is_none_or(|n| n.eq_ignore_ascii_case(candidate));
        // unqualified names are looked up first in the view's own namespace
        let mut candidates: Vec<Relation> = vec![];
        for t in &self.schema.tables {
            if t.name.eq_ignore_ascii_case(rel) && matches_ns(&t.namespace) {
                candidates.push(Relation {
                    id: t.stable_id.clone(),
                    name: format!("{}.{}", t.namespace, t.name),
                    columns: t.columns.iter().map(|c| c.name.clone()).collect(),
                });
            }
        }
        for v in &self.schema.views {
            if v.name.eq_ignore_ascii_case(rel) && matches_ns(&v.namespace) {
                candidates.push(Relation {
                    id: v.stable_id.clone(),
                    name: format!("{}.{}", v.namespace, v.name),
                    columns: v.columns.iter().map(|c| c.name.clone()).collect(),
                });
            }
        }
        if candidates.len() > 1 && ns.is_none() {
            candidates.retain(|c| c.name.starts_with(&format!("{view_ns}.")));
        }
        (candidates.len() == 1).then(|| candidates.remove(0))
    }
}

pub fn table_lineage(schema: &DatabaseSchema, view: &View) -> TableLineage {
    let cat = Catalog { schema };
    let mut notes = vec![];
    let native: BTreeSet<StableId> = schema
        .dependencies
        .iter()
        .filter(|d| d.from == view.stable_id)
        .map(|d| d.to.clone())
        .collect();

    let mut parsed: BTreeSet<StableId> = BTreeSet::new();
    let mut parse_ok = false;
    let mut unresolved = false;
    if let Some(def) = &view.definition {
        match parse_view_query(def, schema.engine) {
            Ok(q) => {
                parse_ok = true;
                let mut rels = vec![];
                let _ = visit_relations(&q, |r| {
                    rels.push(r.clone());
                    ControlFlow::<()>::Continue(())
                });
                let cte_names: BTreeSet<String> = q
                    .with
                    .iter()
                    .flat_map(|w| {
                        w.cte_tables
                            .iter()
                            .map(|c| c.alias.name.value.to_lowercase())
                    })
                    .collect();
                for r in rels {
                    let single = r.0.len() == 1
                        && r.0[0]
                            .as_ident()
                            .is_some_and(|i| cte_names.contains(&i.value.to_lowercase()));
                    if single {
                        continue; // reference to a CTE defined in the same query
                    }
                    match cat.resolve(&r, &view.namespace) {
                        Some(rel) => {
                            parsed.insert(rel.id);
                        }
                        None => {
                            unresolved = true;
                            notes.push(format!("`{r}` in the definition could not be resolved to a known table or view"));
                        }
                    }
                }
            }
            Err(e) => notes.push(format!(
                "definition could not be parsed ({e}); using catalog dependencies only"
            )),
        }
    } else {
        notes
            .push("the engine did not return a definition; using catalog dependencies only".into());
    }

    let all: BTreeSet<&StableId> = native.iter().chain(parsed.iter()).collect();
    let sources: Vec<TableSource> = all
        .into_iter()
        .map(|id| TableSource {
            id: id.clone(),
            name: schema
                .tables
                .iter()
                .find(|t| &t.stable_id == id)
                .map(|t| format!("{}.{}", t.namespace, t.name))
                .or_else(|| {
                    schema
                        .views
                        .iter()
                        .find(|v| &v.stable_id == id)
                        .map(|v| format!("{}.{}", v.namespace, v.name))
                })
                .unwrap_or_else(|| id.to_string()),
            origin: match (native.contains(id), parsed.contains(id)) {
                (true, true) => Origin::Both,
                (true, false) => Origin::Catalog,
                _ => Origin::Parser,
            },
        })
        .collect();

    let agree = parse_ok && !unresolved && native == parsed;
    // SQLite has no dependency catalog, so its "native" set is itself heuristic: parser agreement is enough.
    let confidence = if agree
        || (parse_ok
            && !unresolved
            && native.is_empty()
            && schema.engine != Engine::Postgres
            && !parsed.is_empty())
    {
        Confidence::Complete
    } else {
        if parse_ok && !unresolved && native != parsed {
            notes.push("catalog dependencies and the parsed definition disagree".into());
        }
        Confidence::Partial
    };
    TableLineage {
        view: view.stable_id.clone(),
        view_name: format!("{}.{}", view.namespace, view.name),
        sources,
        confidence,
        notes,
    }
}

pub fn column_lineage(schema: &DatabaseSchema, view: &View) -> ViewColumnLineage {
    let mut notes = vec![];
    let mut out = ViewColumnLineage {
        view: view.stable_id.clone(),
        view_name: format!("{}.{}", view.namespace, view.name),
        columns: vec![],
        confidence: Confidence::Partial,
        notes: vec![],
    };
    let Some(def) = &view.definition else {
        out.notes
            .push("the engine did not return a definition".into());
        out.columns = view
            .columns
            .iter()
            .map(|c| unknown(&c.name, "no definition available"))
            .collect();
        return out;
    };
    let query = match parse_view_query(def, schema.engine) {
        Ok(q) => q,
        Err(e) => {
            out.notes
                .push(format!("definition could not be parsed: {e}"));
            out.columns = view
                .columns
                .iter()
                .map(|c| unknown(&c.name, "definition could not be parsed"))
                .collect();
            return out;
        }
    };
    let cat = Catalog { schema };
    let has_cte = query.with.is_some();
    if has_cte {
        notes.push("the view uses CTEs; columns read through a CTE are reported as UNKNOWN".into());
    }

    let mut branches: Vec<Vec<ColumnLineage>> = vec![];
    collect_branches(
        &query.body,
        &cat,
        &view.namespace,
        &mut branches,
        &mut notes,
    );
    let mut complete = !has_cte && !branches.is_empty();

    let mut columns: Vec<ColumnLineage> = vec![];
    if branches.len() == 1 {
        columns = branches.remove(0);
    } else if branches.len() > 1 {
        // set operation: output columns map by position; every branch contributes
        notes
            .push("set operation (UNION/INTERSECT/EXCEPT): columns are matched by position".into());
        let width = branches[0].len();
        for i in 0..width {
            let mut sources: BTreeSet<ColumnSource> = BTreeSet::new();
            let mut any_unknown = false;
            for b in &branches {
                match b.get(i) {
                    Some(c) => {
                        any_unknown |= c.class == ColumnClass::Unknown;
                        sources.extend(c.sources.iter().cloned());
                    }
                    None => any_unknown = true,
                }
            }
            columns.push(ColumnLineage {
                column: branches[0][i].column.clone(),
                class: if any_unknown {
                    ColumnClass::Unknown
                } else {
                    ColumnClass::Derived
                },
                sources: sources.into_iter().collect(),
                expression: None,
                note: Some("combined from several SELECTs".into()),
            });
        }
        complete = false;
    }

    // Align names with the catalog's own column list when counts match (aliases from `CREATE VIEW v(a,b)`).
    if columns.len() == view.columns.len() {
        for (c, real) in columns.iter_mut().zip(&view.columns) {
            if c.column != real.name {
                c.note = Some(format!("exposed as `{}`", real.name));
                c.column = real.name.clone();
            }
        }
    } else {
        notes.push(format!(
            "parsed {} output columns but the catalog reports {}",
            columns.len(),
            view.columns.len()
        ));
        complete = false;
    }
    if columns.iter().any(|c| c.class == ColumnClass::Unknown) {
        complete = false;
    }
    out.columns = columns;
    out.notes = notes;
    out.confidence = if complete {
        Confidence::Complete
    } else {
        Confidence::Partial
    };
    out
}

fn unknown(name: &str, note: &str) -> ColumnLineage {
    ColumnLineage {
        column: name.into(),
        class: ColumnClass::Unknown,
        sources: vec![],
        expression: None,
        note: Some(note.into()),
    }
}

fn collect_branches(
    body: &SetExpr,
    cat: &Catalog,
    view_ns: &str,
    out: &mut Vec<Vec<ColumnLineage>>,
    notes: &mut Vec<String>,
) {
    match body {
        SetExpr::Select(s) => out.push(select_lineage(s, cat, view_ns, notes)),
        SetExpr::Query(q) => collect_branches(&q.body, cat, view_ns, out, notes),
        SetExpr::SetOperation { left, right, .. } => {
            collect_branches(left, cat, view_ns, out, notes);
            collect_branches(right, cat, view_ns, out, notes);
        }
        _ => notes.push("unsupported query shape; lineage unknown".into()),
    }
}

struct Scope {
    /// alias (lowercase) -> relation
    by_alias: BTreeMap<String, Relation>,
    order: Vec<String>,
}

fn build_scope(select: &Select, cat: &Catalog, view_ns: &str, notes: &mut Vec<String>) -> Scope {
    let mut scope = Scope {
        by_alias: BTreeMap::new(),
        order: vec![],
    };
    let add = |factor: &TableFactor, scope: &mut Scope, notes: &mut Vec<String>| match factor {
        TableFactor::Table { name, alias, .. } => {
            let alias_name = alias
                .as_ref()
                .map(|a| a.name.value.to_lowercase())
                .unwrap_or_else(|| {
                    name.0
                        .last()
                        .and_then(|p| p.as_ident())
                        .map(|i| i.value.to_lowercase())
                        .unwrap_or_default()
                });
            match cat.resolve(name, view_ns) {
                Some(rel) => {
                    scope.order.push(alias_name.clone());
                    scope.by_alias.insert(alias_name, rel);
                }
                None => notes.push(format!(
                    "`{name}` is not a known table or view (CTE or external object?)"
                )),
            }
        }
        TableFactor::Derived { alias, .. } => notes.push(format!(
            "subquery{} in FROM is not traced; its columns are UNKNOWN",
            alias
                .as_ref()
                .map(|a| format!(" `{}`", a.name.value))
                .unwrap_or_default()
        )),
        other => notes.push(format!(
            "FROM item not traced: {}",
            other.to_string().chars().take(60).collect::<String>()
        )),
    };
    for twj in &select.from {
        add(&twj.relation, &mut scope, notes);
        for j in &twj.joins {
            add(&j.relation, &mut scope, notes);
        }
    }
    scope
}

fn select_lineage(
    select: &Select,
    cat: &Catalog,
    view_ns: &str,
    notes: &mut Vec<String>,
) -> Vec<ColumnLineage> {
    let scope = build_scope(select, cat, view_ns, notes);
    let mut cols = vec![];
    for item in &select.projection {
        match item {
            SelectItem::UnnamedExpr(e) => cols.push(expr_lineage(e, None, &scope)),
            SelectItem::ExprWithAlias { expr, alias } => {
                cols.push(expr_lineage(expr, Some(alias.value.clone()), &scope))
            }
            SelectItem::Wildcard(_) => {
                for alias in &scope.order {
                    expand(&scope.by_alias[alias], &mut cols);
                }
                if scope.order.is_empty() {
                    cols.push(unknown(
                        "*",
                        "wildcard over sources that could not be resolved",
                    ));
                }
            }
            SelectItem::QualifiedWildcard(SelectItemQualifiedWildcardKind::ObjectName(n), _) => {
                let a =
                    n.0.last()
                        .and_then(|p| p.as_ident())
                        .map(|i| i.value.to_lowercase())
                        .unwrap_or_default();
                match scope.by_alias.get(&a) {
                    Some(rel) => expand(rel, &mut cols),
                    None => cols.push(unknown(
                        &format!("{n}.*"),
                        "wildcard over an unresolved source",
                    )),
                }
            }
            other => cols.push(unknown(&other.to_string(), "select item not supported")),
        }
    }
    cols
}

fn expand(rel: &Relation, out: &mut Vec<ColumnLineage>) {
    for c in &rel.columns {
        out.push(ColumnLineage {
            column: c.clone(),
            class: ColumnClass::Direct,
            sources: vec![ColumnSource {
                table: rel.id.clone(),
                table_name: rel.name.clone(),
                column: c.clone(),
            }],
            expression: None,
            note: None,
        });
    }
}

fn is_aggregate(e: &Expr) -> bool {
    let mut found = false;
    let _ = visit_expressions(e, |x| {
        if let Expr::Function(f) = x {
            let name = f
                .name
                .0
                .last()
                .and_then(|p| p.as_ident())
                .map(|i| i.value.to_lowercase())
                .unwrap_or_default();
            // window functions are per-row, not aggregations of the output
            if AGGREGATES.contains(&name.as_str()) && f.over.is_none() {
                found = true;
            }
        }
        ControlFlow::<()>::Continue(())
    });
    found
}

fn resolve_ident(parts: &[String], scope: &Scope) -> Result<ColumnSource, String> {
    match parts {
        [col] => {
            let hits: Vec<(&String, &Relation)> = scope
                .by_alias
                .iter()
                .filter(|(_, r)| r.columns.iter().any(|c| c.eq_ignore_ascii_case(col)))
                .collect();
            match hits.as_slice() {
                [(_, r)] => Ok(source(r, col)),
                [] => Err(format!("column `{col}` was not found in any traced source")),
                _ => Err(format!(
                    "column `{col}` is ambiguous between several sources"
                )),
            }
        }
        [.., alias, col] => match scope.by_alias.get(&alias.to_lowercase()) {
            Some(r) if r.columns.iter().any(|c| c.eq_ignore_ascii_case(col)) => Ok(source(r, col)),
            Some(_) => Err(format!("`{alias}` has no column `{col}`")),
            None => Err(format!("`{alias}` is not a traced source")),
        },
        [] => Err("empty identifier".into()),
    }
}

fn source(r: &Relation, col: &str) -> ColumnSource {
    let real = r
        .columns
        .iter()
        .find(|c| c.eq_ignore_ascii_case(col))
        .cloned()
        .unwrap_or_else(|| col.to_string());
    ColumnSource {
        table: r.id.clone(),
        table_name: r.name.clone(),
        column: real,
    }
}

fn ident_parts(e: &Expr) -> Option<Vec<String>> {
    match e {
        Expr::Identifier(i) => Some(vec![i.value.clone()]),
        Expr::CompoundIdentifier(p) => Some(p.iter().map(|i| i.value.clone()).collect()),
        Expr::Nested(inner) => ident_parts(inner),
        _ => None,
    }
}

fn expr_lineage(e: &Expr, alias: Option<String>, scope: &Scope) -> ColumnLineage {
    let text = e.to_string();
    let name = alias
        .clone()
        .or_else(|| ident_parts(e).and_then(|p| p.last().cloned()))
        .unwrap_or_else(|| text.clone());

    // plain column reference
    if let Some(parts) = ident_parts(e) {
        return match resolve_ident(&parts, scope) {
            Ok(src) => ColumnLineage {
                column: name,
                class: ColumnClass::Direct,
                sources: vec![src],
                expression: None,
                note: None,
            },
            Err(why) => unknown(&name, &why),
        };
    }

    // expression: gather every referenced column
    let mut refs: Vec<Vec<String>> = vec![];
    let mut has_subquery = false;
    let _ = visit_expressions(e, |x| {
        match x {
            Expr::Identifier(i) => refs.push(vec![i.value.clone()]),
            Expr::CompoundIdentifier(p) => refs.push(p.iter().map(|i| i.value.clone()).collect()),
            Expr::Subquery(_) | Expr::Exists { .. } | Expr::InSubquery { .. } => {
                has_subquery = true
            }
            Expr::Function(f) => {
                if matches!(f.args, FunctionArguments::Subquery(_)) {
                    has_subquery = true;
                }
            }
            _ => {}
        }
        ControlFlow::<()>::Continue(())
    });
    if has_subquery {
        return ColumnLineage {
            column: name,
            class: ColumnClass::Unknown,
            sources: vec![],
            expression: Some(text),
            note: Some("contains a subquery; sources are not traced".into()),
        };
    }
    let class = if is_aggregate(e) {
        ColumnClass::Aggregated
    } else {
        ColumnClass::Derived
    };
    let mut sources = BTreeSet::new();
    let mut problems = vec![];
    for r in &refs {
        match resolve_ident(r, scope) {
            Ok(s) => {
                sources.insert(s);
            }
            Err(why) => problems.push(why),
        }
    }
    if !problems.is_empty() {
        // Some inputs cannot be attributed: do not claim a full origin.
        return ColumnLineage {
            column: name,
            class: ColumnClass::Unknown,
            sources: sources.into_iter().collect(),
            expression: Some(text),
            note: Some(problems.join("; ")),
        };
    }
    ColumnLineage {
        column: name,
        class,
        note: sources
            .is_empty()
            .then(|| "constant expression".to_string()),
        sources: sources.into_iter().collect(),
        expression: Some(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_support::*;

    fn col(table: &str, name: &str) -> Column {
        Column {
            stable_id: StableId::child(Engine::Postgres, "column", "public", table, name),
            name: name.into(),
            namespace: "public".into(),
            ordinal: 1,
            data_type: "text".into(),
            native_type: "text".into(),
            nullable: true,
            default: None,
            generated: None,
            identity: None,
            comment: None,
            engine_specific_metadata: EngineMeta::new(),
        }
    }

    fn schema_with_view(def: &str, view_cols: &[&str]) -> (DatabaseSchema, View) {
        let mut s = sample();
        for (t, cols) in [
            ("users", vec!["id", "email"]),
            ("orders", vec!["id", "user_id", "total"]),
        ] {
            s.tables.iter_mut().find(|x| x.name == t).unwrap().columns =
                cols.iter().map(|c| col(t, c)).collect();
        }
        s.views
            .retain(|v| v.name != "v_orders" && v.name != "v_top");
        let mut v = view("v_report");
        v.definition = Some(def.into());
        v.columns = view_cols.iter().map(|c| col("v_report", c)).collect();
        s.views.push(v.clone());
        s.dependencies
            .retain(|d| d.kind == DependencyKind::ForeignKey);
        for t in ["users", "orders"] {
            if def.contains(t) {
                s.dependencies.push(Dependency {
                    from: v.stable_id.clone(),
                    to: tid(t),
                    kind: DependencyKind::DependsOn,
                });
            }
        }
        (s, v)
    }

    #[test]
    fn classifies_direct_derived_and_aggregated_columns() {
        let (s, v) = schema_with_view(
            "SELECT u.id, u.email AS mail, lower(u.email) AS lowered, count(o.id) AS n, sum(o.total) + 1 AS t, 1 AS one \
             FROM users u JOIN orders o ON o.user_id = u.id GROUP BY u.id, u.email",
            &["id", "mail", "lowered", "n", "t", "one"],
        );
        let l = column_lineage(&s, &v);
        let by: BTreeMap<&str, &ColumnLineage> =
            l.columns.iter().map(|c| (c.column.as_str(), c)).collect();
        assert_eq!(by["id"].class, ColumnClass::Direct);
        assert_eq!(by["id"].sources[0].column, "id");
        assert_eq!(by["id"].sources[0].table_name, "public.users");
        assert_eq!(by["mail"].class, ColumnClass::Direct);
        assert_eq!(by["mail"].sources[0].column, "email");
        assert_eq!(by["lowered"].class, ColumnClass::Derived);
        assert_eq!(by["n"].class, ColumnClass::Aggregated);
        assert_eq!(by["t"].class, ColumnClass::Aggregated);
        assert_eq!(by["one"].class, ColumnClass::Derived);
        assert_eq!(by["one"].note.as_deref(), Some("constant expression"));
        assert_eq!(l.confidence, Confidence::Complete);
    }

    #[test]
    fn ambiguous_unqualified_column_is_unknown_not_guessed() {
        let (s, v) = schema_with_view(
            "SELECT id FROM users u JOIN orders o ON o.user_id = u.id",
            &["id"],
        );
        let l = column_lineage(&s, &v);
        assert_eq!(l.columns[0].class, ColumnClass::Unknown);
        assert!(l.columns[0].note.as_deref().unwrap().contains("ambiguous"));
        assert!(l.columns[0].sources.is_empty());
        assert_eq!(l.confidence, Confidence::Partial);
    }

    #[test]
    fn wildcard_expands_known_sources() {
        let (s, v) = schema_with_view("SELECT * FROM users", &["id", "email"]);
        let l = column_lineage(&s, &v);
        assert_eq!(l.columns.len(), 2);
        assert!(l.columns.iter().all(|c| c.class == ColumnClass::Direct));
        assert_eq!(l.confidence, Confidence::Complete);
    }

    #[test]
    fn subquery_and_cte_columns_are_unknown() {
        let (s, v) = schema_with_view(
            "SELECT (SELECT max(total) FROM orders) AS m, u.id FROM users u",
            &["m", "id"],
        );
        let l = column_lineage(&s, &v);
        assert_eq!(l.columns[0].class, ColumnClass::Unknown);
        assert_eq!(l.columns[1].class, ColumnClass::Direct);

        let (s, v) = schema_with_view("WITH x AS (SELECT id FROM users) SELECT id FROM x", &["id"]);
        let l = column_lineage(&s, &v);
        assert_eq!(l.columns[0].class, ColumnClass::Unknown);
        assert_eq!(l.confidence, Confidence::Partial);
    }

    #[test]
    fn union_branches_are_merged_by_position() {
        let (s, v) = schema_with_view(
            "SELECT id FROM users UNION ALL SELECT user_id FROM orders",
            &["id"],
        );
        let l = column_lineage(&s, &v);
        assert_eq!(l.columns[0].class, ColumnClass::Derived);
        assert_eq!(l.columns[0].sources.len(), 2);
    }

    #[test]
    fn unparseable_definition_yields_unknown_columns() {
        let (s, v) = schema_with_view("SELEKT nonsense FROM", &["a"]);
        let l = column_lineage(&s, &v);
        assert_eq!(l.columns[0].class, ColumnClass::Unknown);
        let t = table_lineage(&s, &v);
        assert_eq!(t.confidence, Confidence::Partial);
        assert!(t.notes[0].contains("could not be parsed"));
    }

    #[test]
    fn table_lineage_complete_when_parser_and_catalog_agree() {
        let (s, v) = schema_with_view(
            "SELECT u.id FROM users u JOIN orders o ON o.user_id = u.id",
            &["id"],
        );
        let t = table_lineage(&s, &v);
        assert_eq!(t.confidence, Confidence::Complete);
        assert_eq!(t.sources.len(), 2);
        assert!(t.sources.iter().all(|x| x.origin == Origin::Both));
    }

    #[test]
    fn table_lineage_partial_when_catalog_and_parser_disagree() {
        let (mut s, v) = schema_with_view("SELECT u.id FROM users u", &["id"]);
        s.dependencies.push(Dependency {
            from: v.stable_id.clone(),
            to: tid("lonely"),
            kind: DependencyKind::DependsOn,
        });
        let t = table_lineage(&s, &v);
        assert_eq!(t.confidence, Confidence::Partial);
        assert!(t.sources.iter().any(|x| x.origin == Origin::Catalog));
        assert!(t.notes.iter().any(|n| n.contains("disagree")));
    }

    #[test]
    fn unresolved_relations_make_lineage_partial() {
        let (s, v) = schema_with_view("SELECT 1 FROM users u, external_table e", &["x"]);
        let t = table_lineage(&s, &v);
        assert_eq!(t.confidence, Confidence::Partial);
        assert!(t.notes.iter().any(|n| n.contains("external_table")));
    }
}
