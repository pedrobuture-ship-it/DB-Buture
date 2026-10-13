//! Query plan normalization (EXPLAIN output of the three engines) and heuristic observations.
//!
//! Plans are estimates: DB-BUTURE never runs `EXPLAIN ANALYZE`, so nothing here is a measured
//! cost and every observation is labelled as a heuristic.

use serde::Serialize;
use serde_json::Value;

use crate::analysis::{Evidence, Severity};
use crate::error::{AppError, AppResult};
use crate::metadata::Engine;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    pub id: usize,
    pub node_type: String,
    pub relation: Option<String>,
    pub index: Option<String>,
    pub filter: Option<String>,
    pub estimated_cost: Option<f64>,
    pub startup_cost: Option<f64>,
    pub estimated_rows: Option<f64>,
    /// Engine specific attributes that do not fit the common fields.
    pub details: Vec<(String, String)>,
    pub children: Vec<PlanNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub node_id: usize,
    pub severity: Severity,
    pub evidence: Evidence,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPlan {
    pub engine: Engine,
    pub root: PlanNode,
    pub total_cost: Option<f64>,
    pub observations: Vec<Observation>,
    pub notes: Vec<String>,
}

const ESTIMATE_NOTE: &str = "Estimated plan only (no ANALYZE): costs and row counts come from planner statistics, not from running the query.";
const SEQ_SCAN_ROWS: f64 = 10_000.0;

struct Counter(usize);
impl Counter {
    fn next(&mut self) -> usize {
        self.0 += 1;
        self.0
    }
}

fn str_of(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(str::to_string)
}

fn num_of(v: &Value, k: &str) -> Option<f64> {
    match v.get(k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

// ---------- PostgreSQL ----------

pub fn parse_postgres(json: &str) -> AppResult<QueryPlan> {
    let v: Value = serde_json::from_str(json).map_err(|e| AppError::Introspection(format!("invalid EXPLAIN JSON: {e}")))?;
    let plan = v
        .get(0)
        .and_then(|x| x.get("Plan"))
        .ok_or_else(|| AppError::Introspection("EXPLAIN output has no Plan".into()))?;
    let mut c = Counter(0);
    let root = pg_node(plan, &mut c);
    let mut p = QueryPlan { engine: Engine::Postgres, total_cost: root.estimated_cost, root, observations: vec![], notes: vec![ESTIMATE_NOTE.into()] };
    observe(&mut p);
    Ok(p)
}

fn pg_node(v: &Value, c: &mut Counter) -> PlanNode {
    let id = c.next();
    let schema = str_of(v, "Schema");
    let relation = str_of(v, "Relation Name").map(|r| match schema {
        Some(s) => format!("{s}.{r}"),
        None => r,
    });
    let filter = str_of(v, "Filter").or_else(|| str_of(v, "Index Cond")).or_else(|| str_of(v, "Hash Cond")).or_else(|| str_of(v, "Join Filter"));
    let mut details = vec![];
    for k in ["Join Type", "Scan Direction", "Sort Key", "Group Key", "Parent Relationship", "Strategy", "Alias", "Index Cond", "Recheck Cond", "Hash Cond", "Merge Cond", "Subplan Name"] {
        if let Some(x) = v.get(k) {
            let text = match x {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            details.push((k.to_string(), text));
        }
    }
    let children = v.get("Plans").and_then(|p| p.as_array()).map(|a| a.iter().map(|x| pg_node(x, c)).collect()).unwrap_or_default();
    PlanNode {
        id,
        node_type: str_of(v, "Node Type").unwrap_or_else(|| "Unknown".into()),
        relation,
        index: str_of(v, "Index Name"),
        filter,
        estimated_cost: num_of(v, "Total Cost"),
        startup_cost: num_of(v, "Startup Cost"),
        estimated_rows: num_of(v, "Plan Rows"),
        details,
        children,
    }
}

// ---------- MySQL ----------

pub fn parse_mysql(json: &str) -> AppResult<QueryPlan> {
    let v: Value = serde_json::from_str(json).map_err(|e| AppError::Introspection(format!("invalid EXPLAIN JSON: {e}")))?;
    let block = v.get("query_block").ok_or_else(|| AppError::Introspection("EXPLAIN output has no query_block".into()))?;
    let mut c = Counter(0);
    let mut root = PlanNode {
        id: c.next(),
        node_type: "Query block".into(),
        relation: None,
        index: None,
        filter: None,
        estimated_cost: block.get("cost_info").and_then(|ci| num_of(ci, "query_cost")),
        startup_cost: None,
        estimated_rows: None,
        details: vec![],
        children: vec![],
    };
    root.children = mysql_children(block, &mut c);
    let mut p = QueryPlan { engine: Engine::Mysql, total_cost: root.estimated_cost, root, observations: vec![], notes: vec![ESTIMATE_NOTE.into()] };
    observe(&mut p);
    Ok(p)
}

fn mysql_children(v: &Value, c: &mut Counter) -> Vec<PlanNode> {
    let mut out = vec![];
    let Some(obj) = v.as_object() else { return out };
    // keys that wrap further plan content, in the order MySQL nests them
    for (key, val) in obj {
        match key.as_str() {
            "table" => out.push(mysql_table(val, c)),
            "nested_loop" => {
                if let Some(items) = val.as_array() {
                    let mut join = PlanNode { id: c.next(), node_type: "Nested loop".into(), relation: None, index: None, filter: None, estimated_cost: None, startup_cost: None, estimated_rows: None, details: vec![], children: vec![] };
                    for it in items {
                        join.children.extend(mysql_children(it, c));
                    }
                    out.push(join);
                }
            }
            "ordering_operation" | "grouping_operation" | "duplicates_removal" | "buffer_result" | "windowing" => {
                let mut n = PlanNode { id: c.next(), node_type: key.replace('_', " "), relation: None, index: None, filter: None, estimated_cost: None, startup_cost: None, estimated_rows: None, details: vec![], children: vec![] };
                for flag in ["using_filesort", "using_temporary_table"] {
                    if val.get(flag).and_then(|f| f.as_bool()) == Some(true) {
                        n.details.push((flag.to_string(), "true".into()));
                    }
                }
                n.children = mysql_children(val, c);
                out.push(n);
            }
            "union_result" => {
                let mut n = PlanNode { id: c.next(), node_type: "Union".into(), relation: None, index: None, filter: None, estimated_cost: None, startup_cost: None, estimated_rows: None, details: vec![], children: vec![] };
                if let Some(specs) = val.get("query_specifications").and_then(|s| s.as_array()) {
                    for s in specs {
                        if let Some(b) = s.get("query_block") {
                            n.children.extend(mysql_children(b, c));
                        }
                    }
                }
                out.push(n);
            }
            _ => {}
        }
    }
    out
}

fn mysql_table(t: &Value, c: &mut Counter) -> PlanNode {
    let access = str_of(t, "access_type").unwrap_or_default();
    let mut details = vec![("access_type".to_string(), access.clone())];
    if let Some(k) = t.get("possible_keys") {
        details.push(("possible_keys".into(), k.to_string()));
    }
    if let Some(u) = t.get("using_index").and_then(|u| u.as_bool()) {
        details.push(("using_index".into(), u.to_string()));
    }
    let cost = t.get("cost_info");
    PlanNode {
        id: c.next(),
        node_type: match access.as_str() {
            "ALL" => "Full table scan".into(),
            "index" => "Full index scan".into(),
            "range" => "Index range scan".into(),
            "ref" | "eq_ref" | "const" | "system" | "ref_or_null" => "Index lookup".into(),
            other if other.is_empty() => "Table".into(),
            other => format!("Access {other}"),
        },
        relation: str_of(t, "table_name"),
        index: str_of(t, "key"),
        filter: str_of(t, "attached_condition"),
        estimated_cost: cost.and_then(|ci| num_of(ci, "prefix_cost")),
        startup_cost: None,
        estimated_rows: num_of(t, "rows_examined_per_scan").or_else(|| num_of(t, "rows_produced_per_join")),
        details,
        children: vec![],
    }
}

// ---------- SQLite ----------

/// `rows` are `(id, parent, detail)` triples from `EXPLAIN QUERY PLAN`.
pub fn parse_sqlite(rows: &[(i64, i64, String)]) -> AppResult<QueryPlan> {
    if rows.is_empty() {
        return Err(AppError::Introspection("EXPLAIN QUERY PLAN returned no rows".into()));
    }
    let mut c = Counter(0);
    let mut root = PlanNode { id: c.next(), node_type: "Query".into(), relation: None, index: None, filter: None, estimated_cost: None, startup_cost: None, estimated_rows: None, details: vec![], children: vec![] };
    fn build(parent: i64, rows: &[(i64, i64, String)], c: &mut Counter) -> Vec<PlanNode> {
        rows.iter()
            .filter(|(_, p, _)| *p == parent)
            .map(|(id, _, detail)| {
                let mut n = sqlite_node(detail, c);
                n.children = build(*id, rows, c);
                n
            })
            .collect()
    }
    root.children = build(0, rows, &mut c);
    let mut p = QueryPlan {
        engine: Engine::Sqlite,
        total_cost: None,
        root,
        observations: vec![],
        notes: vec!["SQLite does not report costs or row estimates.".into()],
    };
    observe(&mut p);
    Ok(p)
}

fn sqlite_node(detail: &str, c: &mut Counter) -> PlanNode {
    let up = detail.to_uppercase();
    let (kind, rest) = if let Some(r) = detail.strip_prefix("SEARCH ") {
        ("Index search", Some(r))
    } else if let Some(r) = detail.strip_prefix("SCAN ") {
        ("Full scan", Some(r))
    } else {
        ("Step", None)
    };
    let mut relation = None;
    let mut index = None;
    let mut filter = None;
    if let Some(rest) = rest {
        let rest = rest.strip_prefix("TABLE ").unwrap_or(rest);
        let (rel_part, using) = match rest.find(" USING ") {
            Some(i) => (&rest[..i], Some(&rest[i + 7..])),
            None => (rest, None),
        };
        relation = Some(rel_part.split_whitespace().next().unwrap_or("").to_string()).filter(|s| !s.is_empty());
        if let Some(u) = using {
            let u = u.trim();
            if let Some(idx) = u.strip_prefix("COVERING INDEX ").or_else(|| u.strip_prefix("INDEX ")) {
                let mut parts = idx.splitn(2, ' ');
                index = parts.next().map(str::to_string);
                filter = parts.next().map(|f| f.trim().to_string()).filter(|f| !f.is_empty());
            } else if u.starts_with("INTEGER PRIMARY KEY") {
                index = Some("rowid".into());
                filter = u.strip_prefix("INTEGER PRIMARY KEY").map(|f| f.trim().to_string()).filter(|f| !f.is_empty());
            }
        }
    }
    let mut details = vec![("detail".to_string(), detail.to_string())];
    if up.contains("TEMP B-TREE") {
        details.push(("temp_btree".into(), "true".into()));
    }
    PlanNode {
        id: c.next(),
        node_type: if up.starts_with("USE TEMP B-TREE") { "Temp b-tree".into() } else { kind.into() },
        relation,
        index,
        filter,
        estimated_cost: None,
        startup_cost: None,
        estimated_rows: None,
        details,
        children: vec![],
    }
}

// ---------- observations ----------

fn walk<'a>(n: &'a PlanNode, f: &mut impl FnMut(&'a PlanNode)) {
    f(n);
    for c in &n.children {
        walk(c, f);
    }
}

/// Heuristic observations only: they point at things worth a look, never assert a problem.
pub fn observe(plan: &mut QueryPlan) {
    let mut obs = vec![];
    walk(&plan.root, &mut |n| {
        let rel = n.relation.clone().unwrap_or_else(|| "a table".into());
        let rows = n.estimated_rows.unwrap_or(0.0);
        match (plan.engine, n.node_type.as_str()) {
            (Engine::Postgres, "Seq Scan") | (Engine::Mysql, "Full table scan") if rows >= SEQ_SCAN_ROWS => obs.push(Observation {
                node_id: n.id,
                severity: Severity::Medium,
                evidence: Evidence::Heuristic,
                message: format!(
                    "Full scan over {rel} (~{} estimated rows){}. An index matching the filter columns might help; confirm with real data and statistics.",
                    rows as i64,
                    n.filter.as_ref().map(|f| format!(" filtered by {f}")).unwrap_or_default()
                ),
            }),
            (Engine::Sqlite, "Full scan") => obs.push(Observation {
                node_id: n.id,
                severity: Severity::Low,
                evidence: Evidence::Heuristic,
                message: format!("SQLite scans all of {rel}. This is fine for small tables; for large ones consider whether an index could be used."),
            }),
            (_, "Sort") | (Engine::Sqlite, "Temp b-tree") => obs.push(Observation {
                node_id: n.id,
                severity: Severity::Info,
                evidence: Evidence::Heuristic,
                message: "A separate sort step is used. An index that already provides this order could avoid it.".into(),
            }),
            _ => {}
        }
        if n.details.iter().any(|(k, v)| (k == "using_filesort" || k == "using_temporary_table") && v == "true") {
            obs.push(Observation {
                node_id: n.id,
                severity: Severity::Low,
                evidence: Evidence::Heuristic,
                message: "MySQL reports a filesort or temporary table here; an index could avoid it.".into(),
            });
        }
    });
    if let Some(r) = plan.root.estimated_rows {
        if r >= 1_000_000.0 {
            obs.push(Observation { node_id: plan.root.id, severity: Severity::Low, evidence: Evidence::Heuristic, message: format!("The plan is estimated to return about {} rows.", r as i64) });
        }
    }
    plan.observations = obs;
}

#[cfg(test)]
mod tests {
    use super::*;

    const PG: &str = r#"[{"Plan": {
        "Node Type": "Hash Join", "Join Type": "Inner", "Startup Cost": 12.5, "Total Cost": 4521.7, "Plan Rows": 25000, "Plan Width": 64,
        "Hash Cond": "(o.user_id = u.id)",
        "Plans": [
          {"Node Type": "Seq Scan", "Parent Relationship": "Outer", "Relation Name": "orders", "Schema": "shop", "Alias": "o",
           "Startup Cost": 0, "Total Cost": 3100.0, "Plan Rows": 120000, "Plan Width": 40, "Filter": "(total > 10)"},
          {"Node Type": "Hash", "Startup Cost": 8.0, "Total Cost": 8.0, "Plan Rows": 100, "Plan Width": 24,
           "Plans": [{"Node Type": "Index Scan", "Relation Name": "users", "Schema": "shop", "Index Name": "users_pkey",
                      "Startup Cost": 0.1, "Total Cost": 7.0, "Plan Rows": 100, "Plan Width": 24, "Index Cond": "(id < 100)"}]}
        ]}}]"#;

    #[test]
    fn parses_postgres_plan_tree() {
        let p = parse_postgres(PG).unwrap();
        assert_eq!(p.total_cost, Some(4521.7));
        assert_eq!(p.root.node_type, "Hash Join");
        assert_eq!(p.root.children.len(), 2);
        let seq = &p.root.children[0];
        assert_eq!(seq.relation.as_deref(), Some("shop.orders"));
        assert_eq!(seq.filter.as_deref(), Some("(total > 10)"));
        assert_eq!(seq.estimated_rows, Some(120000.0));
        let idx = &p.root.children[1].children[0];
        assert_eq!(idx.index.as_deref(), Some("users_pkey"));
        let ids: Vec<usize> = (1..=4).collect();
        let mut seen = vec![];
        walk(&p.root, &mut |n| seen.push(n.id));
        assert_eq!(seen, ids, "ids are unique and depth-first");
    }

    #[test]
    fn large_seq_scan_is_a_labelled_heuristic() {
        let p = parse_postgres(PG).unwrap();
        assert_eq!(p.observations.len(), 1);
        let o = &p.observations[0];
        assert_eq!(o.evidence, Evidence::Heuristic);
        assert!(o.message.contains("shop.orders"));
        assert!(o.message.contains("might help"), "worded as a possibility, not a certainty");
        assert!(p.notes[0].contains("no ANALYZE"));
    }

    #[test]
    fn small_seq_scan_is_not_flagged() {
        let json = r#"[{"Plan": {"Node Type": "Seq Scan", "Relation Name": "t", "Total Cost": 1, "Startup Cost": 0, "Plan Rows": 50, "Plan Width": 4}}]"#;
        assert!(parse_postgres(json).unwrap().observations.is_empty());
    }

    #[test]
    fn rejects_invalid_postgres_json() {
        assert!(parse_postgres("not json").is_err());
        assert!(parse_postgres("[]").is_err());
    }

    #[test]
    fn parses_mysql_nested_loop() {
        let json = r#"{"query_block": {"select_id": 1, "cost_info": {"query_cost": "120.50"},
          "ordering_operation": {"using_filesort": true,
            "nested_loop": [
              {"table": {"table_name": "orders", "access_type": "ALL", "rows_examined_per_scan": 50000, "attached_condition": "(total > 10)", "cost_info": {"prefix_cost": "5020.00"}}},
              {"table": {"table_name": "users", "access_type": "eq_ref", "key": "PRIMARY", "rows_examined_per_scan": 1, "cost_info": {"prefix_cost": "5100.00"}}}
            ]}}}"#;
        let p = parse_mysql(json).unwrap();
        assert_eq!(p.total_cost, Some(120.5));
        let ord = &p.root.children[0];
        assert_eq!(ord.node_type, "ordering operation");
        let join = &ord.children[0];
        assert_eq!(join.node_type, "Nested loop");
        assert_eq!(join.children[0].node_type, "Full table scan");
        assert_eq!(join.children[1].index.as_deref(), Some("PRIMARY"));
        assert!(p.observations.iter().any(|o| o.message.contains("Full scan over orders")));
        assert!(p.observations.iter().any(|o| o.message.contains("filesort")));
    }

    #[test]
    fn parses_mysql_single_table() {
        let json = r#"{"query_block": {"select_id": 1, "cost_info": {"query_cost": "0.35"}, "table": {"table_name": "t", "access_type": "const", "key": "PRIMARY"}}}"#;
        let p = parse_mysql(json).unwrap();
        assert_eq!(p.root.children[0].node_type, "Index lookup");
        assert!(p.observations.is_empty());
    }

    #[test]
    fn parses_sqlite_query_plan_tree() {
        let rows = vec![
            (3, 0, "SEARCH orders USING INDEX idx_orders_user (user_id=?)".to_string()),
            (7, 0, "SCAN users".to_string()),
            (9, 7, "USE TEMP B-TREE FOR ORDER BY".to_string()),
        ];
        let p = parse_sqlite(&rows).unwrap();
        assert_eq!(p.root.children.len(), 2);
        let search = &p.root.children[0];
        assert_eq!(search.node_type, "Index search");
        assert_eq!(search.relation.as_deref(), Some("orders"));
        assert_eq!(search.index.as_deref(), Some("idx_orders_user"));
        assert_eq!(search.filter.as_deref(), Some("(user_id=?)"));
        let scan = &p.root.children[1];
        assert_eq!(scan.node_type, "Full scan");
        assert_eq!(scan.children[0].node_type, "Temp b-tree");
        assert!(p.observations.iter().any(|o| o.message.contains("scans all of users")));
        assert_eq!(p.total_cost, None);
    }

    #[test]
    fn sqlite_primary_key_search() {
        let p = parse_sqlite(&[(2, 0, "SEARCH t USING INTEGER PRIMARY KEY (rowid=?)".into())]).unwrap();
        assert_eq!(p.root.children[0].index.as_deref(), Some("rowid"));
        assert!(parse_sqlite(&[]).is_err());
    }
}
