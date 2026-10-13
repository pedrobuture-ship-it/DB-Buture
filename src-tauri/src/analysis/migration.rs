//! Migration simulator and risk assessment.
//!
//! DDL is applied to a *copy* of the normalized model. Nothing is ever sent to a database.
//! Risk is split into three kinds that carry different levels of certainty:
//!
//! * **structural** - consequences visible in the catalog (dependent objects, broken keys);
//! * **runtime** - heuristics about locking, rewrites and scans, which depend on data volume;
//! * **application** - always unknown: DB-BUTURE cannot see code, ORMs, reports or jobs.

use serde::Serialize;
use sqlparser::ast::{
    AlterColumnOperation, AlterTableOperation, DropBehavior, ObjectType, RenameTableNameKind,
    Statement,
};
use sqlparser::parser::Parser;

use super::diff::{diff_schemas, SchemaDiff};
use super::impact::analyze_impact;
use super::{Evidence, Severity};
use crate::graph::SchemaGraph;
use crate::importer::{import_sql_with_context, split_statements, SqlDialect};
use crate::metadata::text::mentions_identifier;
use crate::metadata::types::normalize_type;
use crate::metadata::*;

const BIG_TABLE_ROWS: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatementStatus {
    Applied,
    /// Understood but not modelled (for example data statements).
    Ignored,
    /// The statement would fail on a real database, or cannot be parsed.
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementResult {
    pub statement_no: usize,
    pub sql: String,
    pub status: StatementStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskKind {
    Structural,
    Runtime,
    Application,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskItem {
    pub kind: RiskKind,
    pub severity: Severity,
    pub evidence: Evidence,
    pub message: String,
    pub object: Option<StableId>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    pub statements: Vec<StatementResult>,
    pub diff: SchemaDiff,
    pub risks: Vec<RiskItem>,
    pub simulated: DatabaseSchema,
    pub notes: Vec<String>,
}

fn preview(s: &str) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > 160 {
        format!("{}…", one.chars().take(160).collect::<String>())
    } else {
        one
    }
}

fn qualify(n: &sqlparser::ast::ObjectName, default_ns: &str) -> (String, String) {
    let parts: Vec<String> =
        n.0.iter()
            .filter_map(|p| p.as_ident())
            .map(|i| i.value.clone())
            .collect();
    match parts.as_slice() {
        [name] => (default_ns.to_string(), name.clone()),
        [.., ns, name] => (ns.clone(), name.clone()),
        [] => (default_ns.to_string(), String::new()),
    }
}

fn find_table<'a>(
    s: &'a mut DatabaseSchema,
    ns: &str,
    name: &str,
    default_ns: &str,
) -> Option<&'a mut Table> {
    // unqualified names match any namespace when unambiguous, otherwise the dialect default
    let exact = s
        .tables
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(name) && t.namespace.eq_ignore_ascii_case(ns));
    let pos = exact.or_else(|| {
        let hits: Vec<usize> = s
            .tables
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.name.eq_ignore_ascii_case(name)
                    && (ns == default_ns || t.namespace.eq_ignore_ascii_case(default_ns))
            })
            .map(|(i, _)| i)
            .collect();
        (hits.len() == 1).then(|| hits[0])
    })?;
    s.tables.get_mut(pos)
}

pub fn simulate(schema: &DatabaseSchema, ddl: &str) -> MigrationReport {
    let dialect = SqlDialect::from_engine(schema.engine);
    let d = dialect.parser_dialect_pub();
    let default_ns = if schema.engine == Engine::Mysql {
        // MySQL's namespace is the connected database
        schema
            .namespaces
            .first()
            .map(|n| n.name.clone())
            .unwrap_or_else(|| schema.database.clone())
    } else {
        dialect.default_ns().to_string()
    };
    let default_ns = default_ns.as_str();

    let before_graph = SchemaGraph::build(schema);
    let mut sim = schema.clone();
    let mut results = vec![];
    let mut risks: Vec<RiskItem> = vec![];

    let statements = match split_statements(ddl, d.as_ref()) {
        Ok(s) => s,
        Err(e) => {
            return MigrationReport {
                statements: vec![StatementResult {
                    statement_no: 0,
                    sql: preview(ddl),
                    status: StatementStatus::Failed,
                    message: format!("could not tokenize: {e}"),
                }],
                diff: diff_schemas(schema, schema),
                risks,
                simulated: sim,
                notes: vec![],
            }
        }
    };

    for (i, text) in statements.iter().enumerate() {
        let no = i + 1;
        let reported = std::cell::Cell::new(0usize);
        let mut push = |status: StatementStatus, message: String| {
            reported.set(reported.get() + 1);
            results.push(StatementResult {
                statement_no: no,
                sql: preview(text),
                status,
                message,
            });
        };
        let parsed = match Parser::parse_sql(d.as_ref(), text) {
            Ok(p) => p,
            Err(e) => {
                push(StatementStatus::Failed, format!("could not parse: {e}"));
                continue;
            }
        };
        for stmt in parsed {
            match stmt {
                Statement::CreateTable(_) | Statement::CreateView(_) => {
                    let r = import_sql_with_context(text, dialect, Some(&sim));
                    let mut applied = false;
                    for t in r.schema.tables {
                        if sim.tables.iter().any(|x| x.stable_id == t.stable_id) {
                            push(
                                StatementStatus::Failed,
                                format!("{}.{} already exists", t.namespace, t.name),
                            );
                        } else {
                            if !sim.namespaces.iter().any(|n| n.name == t.namespace) {
                                sim.namespaces.push(DatabaseNamespace {
                                    stable_id: StableId::namespace(schema.engine, &t.namespace),
                                    name: t.namespace.clone(),
                                    is_system: false,
                                    engine_specific_metadata: EngineMeta::new(),
                                });
                            }
                            for fk in &t.foreign_keys {
                                sim.dependencies.push(Dependency {
                                    from: fk.source_table.clone(),
                                    to: fk.target_table.clone(),
                                    kind: DependencyKind::ForeignKey,
                                });
                            }
                            push(
                                StatementStatus::Applied,
                                format!("table {}.{} created", t.namespace, t.name),
                            );
                            sim.tables.push(t);
                            applied = true;
                        }
                    }
                    for v in r.schema.views {
                        sim.dependencies.extend(
                            r.schema
                                .dependencies
                                .iter()
                                .filter(|d| d.from == v.stable_id)
                                .cloned(),
                        );
                        push(
                            StatementStatus::Applied,
                            format!("view {}.{} created", v.namespace, v.name),
                        );
                        sim.views.push(v);
                        applied = true;
                    }
                    if !applied && reported.get() == 0 {
                        push(StatementStatus::Ignored, "no effect on the model".into());
                    }
                }
                Statement::CreateIndex(ci) => {
                    let (ns, tname) = qualify(&ci.table_name, default_ns);
                    let Some(t) = find_table(&mut sim, &ns, &tname, default_ns) else {
                        push(
                            StatementStatus::Failed,
                            format!("table {ns}.{tname} does not exist"),
                        );
                        continue;
                    };
                    let mut cols = vec![];
                    let mut exprs = vec![];
                    for c in &ci.columns {
                        match &c.column.expr {
                            sqlparser::ast::Expr::Identifier(id) => cols.push(id.value.clone()),
                            other => exprs.push(other.to_string()),
                        }
                    }
                    if let Some(missing) = cols
                        .iter()
                        .find(|c| !t.columns.iter().any(|x| x.name.eq_ignore_ascii_case(c)))
                    {
                        push(
                            StatementStatus::Failed,
                            format!("column {missing} does not exist on {}", t.name),
                        );
                        continue;
                    }
                    let name = ci
                        .name
                        .as_ref()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| format!("idx_{}_{}", t.name, cols.join("_")));
                    let tid = t.stable_id.clone();
                    let (tns, tn) = (t.namespace.clone(), t.name.clone());
                    let rows = t.estimated_rows;
                    t.indexes.push(Index {
                        stable_id: StableId::object(schema.engine, "index", &tns, &name),
                        name: name.clone(),
                        namespace: tns,
                        table: tid.clone(),
                        columns: cols,
                        unique: ci.unique,
                        primary: false,
                        method: ci
                            .using
                            .as_ref()
                            .map(|u| u.to_string().to_lowercase())
                            .or(Some("btree".into())),
                        predicate: None,
                        expressions: exprs,
                        included_columns: vec![],
                        definition: None,
                        engine_specific_metadata: EngineMeta::new(),
                    });
                    push(
                        StatementStatus::Applied,
                        format!("index {name} created on {tn}"),
                    );
                    if schema.engine == Engine::Postgres && !ci.concurrently {
                        if let Some(rows) = rows.filter(|r| *r >= BIG_TABLE_ROWS / 10) {
                            risks.push(RiskItem { kind: RiskKind::Runtime, severity: Severity::Medium, evidence: Evidence::Heuristic, message: format!("CREATE INDEX without CONCURRENTLY on {tn} (~{rows} rows) blocks writes while it builds."), object: Some(tid) });
                        }
                    }
                }
                Statement::Drop {
                    object_type,
                    names,
                    cascade,
                    if_exists,
                    ..
                } => {
                    for n in &names {
                        let (ns, name) = qualify(n, default_ns);
                        match object_type {
                            ObjectType::Table => {
                                let Some(id) = find_table(&mut sim, &ns, &name, default_ns)
                                    .map(|t| t.stable_id.clone())
                                else {
                                    if !if_exists {
                                        push(
                                            StatementStatus::Failed,
                                            format!("table {ns}.{name} does not exist"),
                                        );
                                    }
                                    continue;
                                };
                                let report = analyze_impact(&sim, &SchemaGraph::build(&sim), &id);
                                let direct: Vec<_> =
                                    report.iter().flat_map(|r| r.direct.iter()).collect();
                                if !direct.is_empty() && !cascade {
                                    let names: Vec<String> =
                                        direct.iter().take(5).map(|i| i.name.clone()).collect();
                                    push(StatementStatus::Failed, format!("cannot drop {name}: {} object(s) depend on it ({}). Use CASCADE to drop them too.", direct.len(), names.join(", ")));
                                    continue;
                                }
                                if let Some(r) = &report {
                                    for item in r.direct.iter().chain(r.transitive.iter()) {
                                        risks.push(RiskItem { kind: RiskKind::Structural, severity: Severity::High, evidence: Evidence::ObservedFact, message: format!("{} {} depends on {name} and would be dropped with it (CASCADE)", format!("{:?}", item.kind).to_lowercase(), item.name), object: Some(item.id.clone()) });
                                    }
                                }
                                remove_table(&mut sim, &id);
                                if cascade {
                                    let gone: Vec<StableId> = report
                                        .iter()
                                        .flat_map(|r| {
                                            r.direct
                                                .iter()
                                                .chain(r.transitive.iter())
                                                .map(|i| i.id.clone())
                                        })
                                        .collect();
                                    sim.views.retain(|v| !gone.contains(&v.stable_id));
                                    sim.triggers.retain(|t| !gone.contains(&t.stable_id));
                                    for t in &mut sim.tables {
                                        t.foreign_keys.retain(|f| f.target_table != id);
                                    }
                                }
                                push(StatementStatus::Applied, format!("table {name} dropped"));
                                risks.push(RiskItem { kind: RiskKind::Application, severity: Severity::High, evidence: Evidence::Unknown, message: format!("Dropping {name} may break application code, ORMs, reports or jobs that use it. DB-BUTURE cannot see those."), object: Some(id) });
                            }
                            ObjectType::View | ObjectType::MaterializedView => {
                                let pos = sim
                                    .views
                                    .iter()
                                    .position(|v| v.name.eq_ignore_ascii_case(&name));
                                match pos {
                                    Some(p) => {
                                        let id = sim.views[p].stable_id.clone();
                                        let dependents = before_graph.dependents(&id, None);
                                        if !dependents.is_empty() && !cascade {
                                            push(StatementStatus::Failed, format!("cannot drop view {name}: {} object(s) depend on it", dependents.len()));
                                            continue;
                                        }
                                        sim.views.remove(p);
                                        sim.dependencies.retain(|d| d.from != id && d.to != id);
                                        push(
                                            StatementStatus::Applied,
                                            format!("view {name} dropped"),
                                        );
                                    }
                                    None if if_exists => {}
                                    None => push(
                                        StatementStatus::Failed,
                                        format!("view {name} does not exist"),
                                    ),
                                }
                            }
                            ObjectType::Index => {
                                let mut found = false;
                                for t in &mut sim.tables {
                                    if let Some(p) = t
                                        .indexes
                                        .iter()
                                        .position(|ix| ix.name.eq_ignore_ascii_case(&name))
                                    {
                                        t.indexes.remove(p);
                                        found = true;
                                        break;
                                    }
                                }
                                if found {
                                    push(StatementStatus::Applied, format!("index {name} dropped"));
                                } else if !if_exists {
                                    push(
                                        StatementStatus::Failed,
                                        format!("index {name} does not exist"),
                                    );
                                }
                            }
                            other => push(
                                StatementStatus::Ignored,
                                format!("DROP {other:?} is not modelled"),
                            ),
                        }
                    }
                }
                Statement::AlterTable(at) => {
                    let (ns, name) = qualify(&at.name, default_ns);
                    for op in &at.operations {
                        alter(
                            &mut sim,
                            &ns,
                            &name,
                            default_ns,
                            op,
                            &mut risks,
                            &mut |st, m| push(st, m),
                            schema.engine,
                        );
                    }
                }
                other => {
                    let kind = format!("{other:?}");
                    let kind = kind
                        .split(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap_or("statement")
                        .to_string();
                    push(
                        StatementStatus::Ignored,
                        format!("{kind} is not modelled by the simulator"),
                    );
                }
            }
        }
    }

    rebuild_dependencies(&mut sim);
    let diff = diff_schemas(schema, &sim);

    // Structural risk for changed/removed columns and tables, from the catalog as it was before.
    for e in &diff.entries {
        if e.change == super::diff::ChangeKind::Added {
            continue;
        }
        if let Some(r) = analyze_impact(schema, &before_graph, &e.id) {
            if r.counts.direct + r.counts.transitive > 0
                && matches!(e.object_type, "column" | "table")
            {
                let sev = match r.level {
                    super::impact::ImpactLevel::High => Severity::High,
                    super::impact::ImpactLevel::Medium => Severity::Medium,
                    _ => Severity::Low,
                };
                risks.push(RiskItem {
                    kind: RiskKind::Structural,
                    severity: sev,
                    evidence: if r.column.is_some() {
                        Evidence::Heuristic
                    } else {
                        Evidence::ObservedFact
                    },
                    message: format!(
                        "{} {} is used by {} direct and {} transitive database object(s)",
                        e.object_type, r.target_name, r.counts.direct, r.counts.transitive
                    ),
                    object: Some(e.id.clone()),
                });
            }
        }
        if matches!(e.object_type, "column" | "table") {
            let removed_or_renamed = e.change == super::diff::ChangeKind::Removed;
            let type_changed = e.fields.iter().any(|f| f.field == "type");
            if removed_or_renamed || type_changed {
                risks.push(RiskItem {
                    kind: RiskKind::Application,
                    severity: Severity::Medium,
                    evidence: Evidence::Unknown,
                    message: format!("{} {} {}: code that reads or writes it (ORMs, reports, jobs) may break. This is outside what DB-BUTURE can see.", e.object_type, e.name, if removed_or_renamed { "is removed or renamed" } else { "changes type" }),
                    object: Some(e.id.clone()),
                });
            }
        }
    }
    risks.sort_by(|a, b| {
        (std::cmp::Reverse(a.severity), a.kind as u8, &a.message).cmp(&(
            std::cmp::Reverse(b.severity),
            b.kind as u8,
            &b.message,
        ))
    });
    risks.dedup_by(|a, b| a.message == b.message);

    MigrationReport {
        statements: results,
        diff,
        risks,
        simulated: sim,
        notes: vec![
            "Simulated on a copy of the model. Nothing was executed against any database.".into(),
        ],
    }
}

fn remove_table(s: &mut DatabaseSchema, id: &StableId) {
    s.tables.retain(|t| &t.stable_id != id);
    s.dependencies.retain(|d| &d.from != id && &d.to != id);
    for t in &mut s.tables {
        t.foreign_keys.retain(|f| &f.target_table != id);
    }
}

/// Dependencies are derived: recompute FK edges from the model, keep view/trigger edges whose ends still exist.
fn rebuild_dependencies(s: &mut DatabaseSchema) {
    let alive: std::collections::HashSet<StableId> = s
        .tables
        .iter()
        .map(|t| t.stable_id.clone())
        .chain(s.views.iter().map(|v| v.stable_id.clone()))
        .chain(s.routines.iter().map(|r| r.stable_id.clone()))
        .chain(s.triggers.iter().map(|t| t.stable_id.clone()))
        .collect();
    s.dependencies.retain(|d| {
        d.kind != DependencyKind::ForeignKey && alive.contains(&d.from) && alive.contains(&d.to)
    });
    for t in &s.tables {
        for fk in &t.foreign_keys {
            s.dependencies.push(Dependency {
                from: fk.source_table.clone(),
                to: fk.target_table.clone(),
                kind: DependencyKind::ForeignKey,
            });
        }
    }
    s.dependencies
        .sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
    s.dependencies.dedup();
}

#[allow(clippy::too_many_arguments)]
fn alter(
    sim: &mut DatabaseSchema,
    ns: &str,
    name: &str,
    default_ns: &str,
    op: &AlterTableOperation,
    risks: &mut Vec<RiskItem>,
    push: &mut dyn FnMut(StatementStatus, String),
    engine: Engine,
) {
    let views_snapshot: Vec<(StableId, String, Option<String>)> = sim
        .views
        .iter()
        .map(|v| (v.stable_id.clone(), v.name.clone(), v.definition.clone()))
        .collect();
    let all_tables: Vec<(StableId, String, String, Vec<ForeignKey>)> = sim
        .tables
        .iter()
        .map(|t| {
            (
                t.stable_id.clone(),
                t.name.clone(),
                t.namespace.clone(),
                t.foreign_keys.clone(),
            )
        })
        .collect();
    let Some(t) = find_table(sim, ns, name, default_ns) else {
        push(
            StatementStatus::Failed,
            format!("table {ns}.{name} does not exist"),
        );
        return;
    };
    let tid = t.stable_id.clone();
    let rows = t.estimated_rows.unwrap_or(0);
    let big = rows >= BIG_TABLE_ROWS;
    match op {
        AlterTableOperation::AddColumn {
            column_def,
            if_not_exists,
            ..
        } => {
            let cname = column_def.name.value.clone();
            if t.columns
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(&cname))
            {
                if !*if_not_exists {
                    push(
                        StatementStatus::Failed,
                        format!("column {cname} already exists on {}", t.name),
                    );
                }
                return;
            }
            let native = column_def.data_type.to_string().to_lowercase();
            let not_null = column_def
                .options
                .iter()
                .any(|o| matches!(o.option, sqlparser::ast::ColumnOption::NotNull));
            let default = column_def.options.iter().find_map(|o| match &o.option {
                sqlparser::ast::ColumnOption::Default(e) => Some(e.to_string()),
                _ => None,
            });
            if not_null && default.is_none() && rows != 0 {
                risks.push(RiskItem { kind: RiskKind::Runtime, severity: Severity::High, evidence: if t.estimated_rows.is_some() { Evidence::Heuristic } else { Evidence::Unknown }, message: format!("ADD COLUMN {cname} NOT NULL without a DEFAULT fails if {} already contains rows.", t.name), object: Some(tid.clone()) });
            }
            if let Some(d) = &default {
                let volatile = [
                    "now()",
                    "random()",
                    "uuid",
                    "clock_timestamp",
                    "current_timestamp",
                    "nextval",
                ]
                .iter()
                .any(|v| d.to_lowercase().contains(v));
                if volatile && big && engine == Engine::Postgres {
                    risks.push(RiskItem { kind: RiskKind::Runtime, severity: Severity::High, evidence: Evidence::Heuristic, message: format!("DEFAULT {d} is volatile, so adding {cname} rewrites all ~{rows} rows of {} under an exclusive lock.", t.name), object: Some(tid.clone()) });
                }
            }
            let ordinal = t.columns.iter().map(|c| c.ordinal).max().unwrap_or(0) + 1;
            t.columns.push(Column {
                stable_id: StableId::child(engine, "column", &t.namespace, &t.name, &cname),
                name: cname.clone(),
                namespace: t.namespace.clone(),
                ordinal,
                data_type: normalize_type(engine, &native),
                native_type: native,
                nullable: !not_null,
                default,
                generated: None,
                identity: None,
                comment: None,
                engine_specific_metadata: EngineMeta::new(),
            });
            push(
                StatementStatus::Applied,
                format!("column {cname} added to {}", t.name),
            );
        }
        AlterTableOperation::DropColumn {
            column_names,
            drop_behavior,
            if_exists,
            ..
        } => {
            let cascade = matches!(drop_behavior, Some(DropBehavior::Cascade));
            for c in column_names {
                let cname = &c.value;
                if !t.columns.iter().any(|x| x.name.eq_ignore_ascii_case(cname)) {
                    if !*if_exists {
                        push(
                            StatementStatus::Failed,
                            format!("column {cname} does not exist on {}", t.name),
                        );
                    }
                    continue;
                }
                let referenced_by: Vec<String> = all_tables
                    .iter()
                    .flat_map(|(_, tn, _, fks)| {
                        fks.iter()
                            .filter(|f| {
                                f.target_table == tid
                                    && f.target_columns
                                        .iter()
                                        .any(|x| x.eq_ignore_ascii_case(cname))
                            })
                            .map(move |f| format!("{tn}.{}", f.name))
                    })
                    .collect();
                let used_by_views: Vec<String> = views_snapshot
                    .iter()
                    .filter(|(_, _, def)| {
                        def.as_deref()
                            .is_some_and(|d| mentions_identifier(d, cname))
                    })
                    .map(|(_, n, _)| n.clone())
                    .collect();
                if (!referenced_by.is_empty() || !used_by_views.is_empty()) && !cascade {
                    let mut why = vec![];
                    if !referenced_by.is_empty() {
                        why.push(format!(
                            "referenced by foreign key(s) {}",
                            referenced_by.join(", ")
                        ));
                    }
                    if !used_by_views.is_empty() {
                        why.push(format!(
                            "possibly used by view(s) {} (text match, heuristic)",
                            used_by_views.join(", ")
                        ));
                    }
                    push(StatementStatus::Failed, format!("cannot drop {}.{cname}: {}. Use CASCADE or change those objects first.", t.name, why.join("; ")));
                    continue;
                }
                t.columns.retain(|x| !x.name.eq_ignore_ascii_case(cname));
                let before = t.indexes.len();
                t.indexes.retain(|ix| {
                    !ix.columns
                        .iter()
                        .chain(ix.included_columns.iter())
                        .any(|x| x.eq_ignore_ascii_case(cname))
                });
                t.foreign_keys.retain(|f| {
                    !f.source_columns
                        .iter()
                        .any(|x| x.eq_ignore_ascii_case(cname))
                });
                t.unique_constraints
                    .retain(|u| !u.columns.iter().any(|x| x.eq_ignore_ascii_case(cname)));
                if t.primary_key
                    .as_ref()
                    .is_some_and(|p| p.columns.iter().any(|x| x.eq_ignore_ascii_case(cname)))
                {
                    t.primary_key = None;
                }
                let dropped_ix = before - t.indexes.len();
                push(
                    StatementStatus::Applied,
                    format!(
                        "column {cname} dropped from {}{}",
                        t.name,
                        if dropped_ix > 0 {
                            format!(" ({dropped_ix} index(es) on it dropped automatically)")
                        } else {
                            String::new()
                        }
                    ),
                );
                risks.push(RiskItem { kind: RiskKind::Runtime, severity: Severity::Medium, evidence: Evidence::ObservedFact, message: format!("Dropping {}.{cname} permanently deletes its data; the change cannot be rolled back without a backup.", t.name), object: Some(tid.clone()) });
            }
        }
        AlterTableOperation::RenameColumn {
            old_column_name,
            new_column_name,
        } => {
            let old = &old_column_name.value;
            if let Some(c) = t
                .columns
                .iter_mut()
                .find(|c| c.name.eq_ignore_ascii_case(old))
            {
                c.name = new_column_name.value.clone();
                c.stable_id = StableId::child(engine, "column", &c.namespace, &t.name, &c.name);
                for ix in &mut t.indexes {
                    for col in &mut ix.columns {
                        if col.eq_ignore_ascii_case(old) {
                            *col = new_column_name.value.clone();
                        }
                    }
                }
                if let Some(pk) = &mut t.primary_key {
                    for col in &mut pk.columns {
                        if col.eq_ignore_ascii_case(old) {
                            *col = new_column_name.value.clone();
                        }
                    }
                }
                push(
                    StatementStatus::Applied,
                    format!("column {old} renamed to {}", new_column_name.value),
                );
                risks.push(RiskItem { kind: RiskKind::Application, severity: Severity::High, evidence: Evidence::Unknown, message: format!("Renaming {}.{old} breaks any code that still uses the old name. DB-BUTURE cannot see that code.", t.name), object: Some(tid.clone()) });
            } else {
                push(
                    StatementStatus::Failed,
                    format!("column {old} does not exist on {}", t.name),
                );
            }
        }
        AlterTableOperation::RenameTable { table_name } => {
            let new = match table_name {
                RenameTableNameKind::As(n) | RenameTableNameKind::To(n) => qualify(n, default_ns).1,
            };
            let old = t.name.clone();
            t.name = new.clone();
            t.stable_id = StableId::table(engine, &t.namespace, &new);
            let new_id = t.stable_id.clone();
            for c in &mut t.columns {
                c.stable_id = StableId::child(engine, "column", &c.namespace, &new, &c.name);
            }
            for fk in &mut sim
                .tables
                .iter_mut()
                .flat_map(|x| x.foreign_keys.iter_mut())
            {
                if fk.target_table == tid {
                    fk.target_table = new_id.clone();
                }
                if fk.source_table == tid {
                    fk.source_table = new_id.clone();
                }
            }
            push(
                StatementStatus::Applied,
                format!("table {old} renamed to {new}"),
            );
            risks.push(RiskItem { kind: RiskKind::Application, severity: Severity::High, evidence: Evidence::Unknown, message: format!("Renaming table {old} breaks any code that still uses the old name. DB-BUTURE cannot see that code."), object: Some(tid) });
        }
        AlterTableOperation::AlterColumn { column_name, op } => {
            let cname = &column_name.value;
            let tname = t.name.clone();
            let Some(c) = t
                .columns
                .iter_mut()
                .find(|c| c.name.eq_ignore_ascii_case(cname))
            else {
                push(
                    StatementStatus::Failed,
                    format!("column {cname} does not exist on {tname}"),
                );
                return;
            };
            match op {
                AlterColumnOperation::SetNotNull => {
                    c.nullable = false;
                    push(StatementStatus::Applied, format!("{cname} set NOT NULL"));
                    risks.push(RiskItem { kind: RiskKind::Runtime, severity: if big { Severity::Medium } else { Severity::Low }, evidence: Evidence::Heuristic, message: format!("SET NOT NULL scans {tname} to validate existing rows and fails if any value is NULL{}", if big { format!(" (~{rows} rows, holds a lock while scanning)") } else { String::new() }), object: Some(tid.clone()) });
                }
                AlterColumnOperation::DropNotNull => {
                    c.nullable = true;
                    push(StatementStatus::Applied, format!("{cname} is now nullable"));
                }
                AlterColumnOperation::SetDefault { value } => {
                    c.default = Some(value.to_string());
                    push(StatementStatus::Applied, format!("{cname} default set"));
                }
                AlterColumnOperation::DropDefault => {
                    c.default = None;
                    push(StatementStatus::Applied, format!("{cname} default dropped"));
                }
                AlterColumnOperation::SetDataType { data_type, .. } => {
                    let from = c.native_type.clone();
                    c.native_type = data_type.to_string().to_lowercase();
                    c.data_type = normalize_type(engine, &c.native_type);
                    push(
                        StatementStatus::Applied,
                        format!("{cname} type changed from {from} to {}", c.native_type),
                    );
                    risks.push(RiskItem { kind: RiskKind::Runtime, severity: if big { Severity::High } else { Severity::Medium }, evidence: Evidence::Heuristic, message: format!("Changing the type of {tname}.{cname} usually rewrites the table and takes an exclusive lock{}. Casting may also fail for incompatible values.", if big { format!(" (~{rows} rows)") } else { String::new() }), object: Some(tid.clone()) });
                }
                other => push(
                    StatementStatus::Ignored,
                    format!("ALTER COLUMN {other} is not modelled"),
                ),
            }
        }
        AlterTableOperation::AddConstraint { constraint, .. } => match constraint {
            sqlparser::ast::TableConstraint::ForeignKey(fk) => {
                let (tns, tn) = qualify(&fk.foreign_table, default_ns);
                let target = all_tables
                    .iter()
                    .find(|(_, n, tns_, _)| {
                        n.eq_ignore_ascii_case(&tn)
                            && (tns_.eq_ignore_ascii_case(&tns) || tns == default_ns)
                    })
                    .map(|(id, _, _, _)| id.clone());
                let Some(target_id) = target else {
                    push(
                        StatementStatus::Failed,
                        format!("referenced table {tn} does not exist"),
                    );
                    return;
                };
                let cname = fk
                    .name
                    .as_ref()
                    .map(|n| n.value.clone())
                    .unwrap_or_else(|| {
                        format!(
                            "{}_{}_fkey",
                            t.name,
                            fk.columns
                                .first()
                                .map(|c| c.value.clone())
                                .unwrap_or_default()
                        )
                    });
                t.foreign_keys.push(ForeignKey {
                    stable_id: StableId::child(engine, "constraint", &t.namespace, &t.name, &cname),
                    name: cname.clone(),
                    namespace: t.namespace.clone(),
                    source_table: tid.clone(),
                    source_columns: fk.columns.iter().map(|c| c.value.clone()).collect(),
                    target_table: target_id,
                    target_columns: fk
                        .referred_columns
                        .iter()
                        .map(|c| c.value.clone())
                        .collect(),
                    on_delete: ReferentialAction::NoAction,
                    on_update: ReferentialAction::NoAction,
                    engine_specific_metadata: EngineMeta::new(),
                });
                push(
                    StatementStatus::Applied,
                    format!("foreign key {cname} added"),
                );
                risks.push(RiskItem { kind: RiskKind::Runtime, severity: if big { Severity::Medium } else { Severity::Low }, evidence: Evidence::Heuristic, message: format!("Adding a foreign key validates every existing row of {}{}. In PostgreSQL, NOT VALID followed by VALIDATE CONSTRAINT reduces locking.", t.name, if big { format!(" (~{rows} rows)") } else { String::new() }), object: Some(tid.clone()) });
            }
            sqlparser::ast::TableConstraint::PrimaryKey(pk) => {
                t.primary_key = Some(PrimaryKey {
                    stable_id: StableId::child(
                        engine,
                        "constraint",
                        &t.namespace,
                        &t.name,
                        &pk.name
                            .as_ref()
                            .map(|n| n.value.clone())
                            .unwrap_or_else(|| format!("{}_pkey", t.name)),
                    ),
                    name: pk
                        .name
                        .as_ref()
                        .map(|n| n.value.clone())
                        .unwrap_or_else(|| format!("{}_pkey", t.name)),
                    namespace: t.namespace.clone(),
                    columns: pk
                        .columns
                        .iter()
                        .map(|c| c.column.expr.to_string())
                        .collect(),
                    engine_specific_metadata: EngineMeta::new(),
                });
                push(StatementStatus::Applied, "primary key added".into());
            }
            sqlparser::ast::TableConstraint::Unique(u) => {
                let cname = u
                    .name
                    .as_ref()
                    .map(|n| n.value.clone())
                    .unwrap_or_else(|| format!("{}_key", t.name));
                t.unique_constraints.push(UniqueConstraint {
                    stable_id: StableId::child(engine, "constraint", &t.namespace, &t.name, &cname),
                    name: cname,
                    namespace: t.namespace.clone(),
                    columns: u
                        .columns
                        .iter()
                        .map(|c| c.column.expr.to_string())
                        .collect(),
                    engine_specific_metadata: EngineMeta::new(),
                });
                push(StatementStatus::Applied, "unique constraint added".into());
                risks.push(RiskItem { kind: RiskKind::Runtime, severity: Severity::Low, evidence: Evidence::Heuristic, message: format!("Adding a unique constraint builds an index over {} and fails if duplicates exist.", t.name), object: Some(tid.clone()) });
            }
            sqlparser::ast::TableConstraint::Check(ch) => {
                let cname = ch
                    .name
                    .as_ref()
                    .map(|n| n.value.clone())
                    .unwrap_or_else(|| format!("{}_check", t.name));
                t.check_constraints.push(CheckConstraint {
                    stable_id: StableId::child(engine, "constraint", &t.namespace, &t.name, &cname),
                    name: cname,
                    namespace: t.namespace.clone(),
                    expression: ch.expr.to_string(),
                    engine_specific_metadata: EngineMeta::new(),
                });
                push(StatementStatus::Applied, "check constraint added".into());
            }
            other => push(
                StatementStatus::Ignored,
                format!("constraint not modelled: {other}"),
            ),
        },
        AlterTableOperation::DropConstraint {
            name: cname,
            if_exists,
            ..
        } => {
            let n = &cname.value;
            let before = t.foreign_keys.len()
                + t.unique_constraints.len()
                + t.check_constraints.len()
                + usize::from(t.primary_key.is_some());
            t.foreign_keys.retain(|f| !f.name.eq_ignore_ascii_case(n));
            t.unique_constraints
                .retain(|u| !u.name.eq_ignore_ascii_case(n));
            t.check_constraints
                .retain(|c| !c.name.eq_ignore_ascii_case(n));
            if t.primary_key
                .as_ref()
                .is_some_and(|p| p.name.eq_ignore_ascii_case(n))
            {
                t.primary_key = None;
            }
            let after = t.foreign_keys.len()
                + t.unique_constraints.len()
                + t.check_constraints.len()
                + usize::from(t.primary_key.is_some());
            if before == after {
                if !*if_exists {
                    push(
                        StatementStatus::Failed,
                        format!("constraint {n} does not exist on {}", t.name),
                    );
                }
            } else {
                push(StatementStatus::Applied, format!("constraint {n} dropped"));
            }
        }
        other => push(
            StatementStatus::Ignored,
            format!(
                "ALTER TABLE operation not modelled: {}",
                preview(&other.to_string())
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_support::*;

    fn col(table: &str, name: &str, ty: &str) -> Column {
        Column {
            stable_id: StableId::child(Engine::Postgres, "column", "public", table, name),
            name: name.into(),
            namespace: "public".into(),
            ordinal: 1,
            data_type: ty.into(),
            native_type: ty.into(),
            nullable: false,
            default: None,
            generated: None,
            identity: None,
            comment: None,
            engine_specific_metadata: EngineMeta::new(),
        }
    }

    fn base() -> DatabaseSchema {
        let mut s = sample();
        for (t, cols) in [
            ("users", vec!["id", "email"]),
            ("orders", vec!["id", "user_id", "total"]),
        ] {
            s.tables.iter_mut().find(|x| x.name == t).unwrap().columns =
                cols.iter().map(|c| col(t, c, "bigint")).collect();
        }
        let users = s.tables.iter_mut().find(|x| x.name == "users").unwrap();
        users.estimated_rows = Some(5_000_000);
        let orders = s.tables.iter_mut().find(|x| x.name == "orders").unwrap();
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
        let v = s.views.iter_mut().find(|v| v.name == "v_orders").unwrap();
        v.definition = Some("SELECT o.id, o.total FROM orders o".into());
        s
    }

    fn statuses(r: &MigrationReport) -> Vec<StatementStatus> {
        r.statements.iter().map(|s| s.status).collect()
    }

    #[test]
    fn add_column_updates_the_copy_not_the_input() {
        let s = base();
        let r = simulate(&s, "ALTER TABLE users ADD COLUMN nickname text;");
        assert_eq!(statuses(&r), [StatementStatus::Applied]);
        assert!(r
            .simulated
            .tables
            .iter()
            .find(|t| t.name == "users")
            .unwrap()
            .columns
            .iter()
            .any(|c| c.name == "nickname"));
        assert!(
            !s.tables
                .iter()
                .find(|t| t.name == "users")
                .unwrap()
                .columns
                .iter()
                .any(|c| c.name == "nickname"),
            "input is untouched"
        );
        assert_eq!(r.diff.summary.added, 1);
        assert!(r.notes[0].contains("Nothing was executed"));
    }

    #[test]
    fn dropping_a_referenced_table_fails_without_cascade() {
        let r = simulate(&base(), "DROP TABLE users;");
        assert_eq!(statuses(&r), [StatementStatus::Failed]);
        assert!(r.statements[0].message.contains("depend on it"));
        assert_eq!(
            r.diff.entries.len(),
            0,
            "a failed statement changes nothing"
        );
    }

    #[test]
    fn cascade_drop_removes_dependents_and_reports_structural_risk() {
        let r = simulate(&base(), "DROP TABLE users CASCADE;");
        assert_eq!(statuses(&r), [StatementStatus::Applied]);
        assert!(!r.simulated.tables.iter().any(|t| t.name == "users"));
        assert!(r
            .simulated
            .tables
            .iter()
            .find(|t| t.name == "orders")
            .unwrap()
            .foreign_keys
            .is_empty());
        assert!(r
            .risks
            .iter()
            .any(|x| x.kind == RiskKind::Structural && x.evidence == Evidence::ObservedFact));
        let app = r
            .risks
            .iter()
            .find(|x| x.kind == RiskKind::Application)
            .unwrap();
        assert_eq!(
            app.evidence,
            Evidence::Unknown,
            "application risk is never presented as known"
        );
    }

    #[test]
    fn drop_column_blocked_by_foreign_key_and_allowed_with_cascade() {
        let r = simulate(&base(), "ALTER TABLE users DROP COLUMN id;");
        assert_eq!(statuses(&r), [StatementStatus::Failed]);
        assert!(r.statements[0].message.contains("orders_user_fk"));
        let r = simulate(&base(), "ALTER TABLE users DROP COLUMN id CASCADE;");
        assert_eq!(statuses(&r), [StatementStatus::Applied]);
    }

    #[test]
    fn drop_column_used_by_view_text_is_flagged_heuristically() {
        let r = simulate(&base(), "ALTER TABLE orders DROP COLUMN total;");
        assert_eq!(statuses(&r), [StatementStatus::Failed]);
        assert!(r.statements[0].message.contains("heuristic"));
    }

    #[test]
    fn type_change_on_big_table_is_a_runtime_heuristic() {
        let r = simulate(&base(), "ALTER TABLE users ALTER COLUMN email TYPE text;");
        assert_eq!(statuses(&r), [StatementStatus::Applied]);
        let rt = r
            .risks
            .iter()
            .find(|x| x.kind == RiskKind::Runtime)
            .unwrap();
        assert_eq!(rt.evidence, Evidence::Heuristic);
        assert!(rt.message.contains("5000000"));
        assert_eq!(r.diff.summary.changed, 1);
    }

    #[test]
    fn not_null_without_default_warns_for_populated_tables() {
        let r = simulate(&base(), "ALTER TABLE users ADD COLUMN age int NOT NULL;");
        assert!(r
            .risks
            .iter()
            .any(|x| x.kind == RiskKind::Runtime && x.message.contains("without a DEFAULT")));
    }

    #[test]
    fn rename_column_is_application_risk() {
        let r = simulate(&base(), "ALTER TABLE users RENAME COLUMN email TO mail;");
        assert_eq!(statuses(&r), [StatementStatus::Applied]);
        assert!(r
            .risks
            .iter()
            .any(|x| x.kind == RiskKind::Application && x.severity == Severity::High));
        assert!(r
            .simulated
            .tables
            .iter()
            .find(|t| t.name == "users")
            .unwrap()
            .columns
            .iter()
            .any(|c| c.name == "mail"));
    }

    #[test]
    fn create_table_may_reference_existing_tables() {
        let r = simulate(
            &base(),
            "CREATE TABLE payments (id bigint PRIMARY KEY, order_id bigint REFERENCES orders(id));",
        );
        assert_eq!(statuses(&r), [StatementStatus::Applied]);
        let p = r
            .simulated
            .tables
            .iter()
            .find(|t| t.name == "payments")
            .unwrap();
        assert_eq!(p.foreign_keys[0].target_table, tid("orders"));
        assert!(r
            .simulated
            .dependencies
            .iter()
            .any(|d| d.from == tid("payments") && d.to == tid("orders")));
    }

    #[test]
    fn create_and_drop_index() {
        let r = simulate(
            &base(),
            "CREATE INDEX idx_orders_user ON orders (user_id); DROP INDEX idx_orders_user;",
        );
        assert_eq!(
            statuses(&r),
            [StatementStatus::Applied, StatementStatus::Applied]
        );
        assert!(
            r.diff.entries.is_empty(),
            "index created then dropped nets out"
        );
    }

    #[test]
    fn unknown_targets_and_garbage_fail_without_panicking() {
        let r = simulate(&base(), "ALTER TABLE ghost ADD COLUMN x int; DROP TABLE nope; SELEKT 1; INSERT INTO users VALUES (1);");
        assert_eq!(
            statuses(&r),
            [
                StatementStatus::Failed,
                StatementStatus::Failed,
                StatementStatus::Failed,
                StatementStatus::Ignored
            ]
        );
        assert!(r.diff.entries.is_empty());
        assert!(simulate(&base(), "").statements.is_empty());
    }

    #[test]
    fn if_exists_is_not_an_error() {
        let r = simulate(&base(), "DROP TABLE IF EXISTS ghost;");
        assert!(
            r.statements.is_empty()
                || r.statements
                    .iter()
                    .all(|s| s.status != StatementStatus::Failed)
        );
    }
}
