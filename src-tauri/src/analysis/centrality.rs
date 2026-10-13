//! Structural centrality of tables over the foreign key graph.
//!
//! Numbers are always paired with a plain-language explanation: "referenced by 7 tables" and
//! "31% of shortest relationship paths pass through it" mean something; a bare score does not.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::Serialize;

use crate::metadata::*;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCentrality {
    pub table: StableId,
    pub name: String,
    /// Distinct tables that reference this one.
    pub referenced_by: usize,
    /// Distinct tables this one references.
    pub references: usize,
    /// Share (0..=1) of shortest paths between other tables that pass through this table.
    pub path_share: f64,
    pub role: Role,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Many tables depend on it.
    Hub,
    /// Connects otherwise distant parts of the schema.
    Bridge,
    /// Neither.
    Regular,
    /// No foreign keys at all.
    Isolated,
}

const HUB_MIN_REFERENCES: usize = 4;
const BRIDGE_MIN_SHARE: f64 = 0.1;

pub fn centrality(schema: &DatabaseSchema) -> Vec<TableCentrality> {
    let ids: Vec<&StableId> = schema.tables.iter().map(|t| &t.stable_id).collect();
    let index: BTreeMap<&StableId, usize> =
        ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let n = ids.len();
    let mut adj: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    let mut referenced_by: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    let mut references: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for t in &schema.tables {
        let Some(&a) = index.get(&t.stable_id) else {
            continue;
        };
        for fk in &t.foreign_keys {
            let Some(&b) = index.get(&fk.target_table) else {
                continue;
            };
            if a == b {
                continue;
            }
            adj[a].insert(b);
            adj[b].insert(a);
            references[a].insert(b);
            referenced_by[b].insert(a);
        }
    }

    // Brandes' algorithm for unweighted, undirected betweenness.
    let mut between = vec![0.0f64; n];
    for s in 0..n {
        let mut stack = vec![];
        let mut preds: Vec<Vec<usize>> = vec![vec![]; n];
        let mut sigma = vec![0.0f64; n];
        let mut dist = vec![-1i64; n];
        sigma[s] = 1.0;
        dist[s] = 0;
        let mut q = VecDeque::from([s]);
        while let Some(v) = q.pop_front() {
            stack.push(v);
            for &w in &adj[v] {
                if dist[w] < 0 {
                    dist[w] = dist[v] + 1;
                    q.push_back(w);
                }
                if dist[w] == dist[v] + 1 {
                    sigma[w] += sigma[v];
                    preds[w].push(v);
                }
            }
        }
        let mut delta = vec![0.0f64; n];
        while let Some(w) = stack.pop() {
            for &v in &preds[w] {
                delta[v] += sigma[v] / sigma[w] * (1.0 + delta[w]);
            }
            if w != s {
                between[w] += delta[w];
            }
        }
    }
    // undirected: every pair counted twice; normalise by the number of pairs not involving the node
    let norm = if n > 2 {
        ((n - 1) * (n - 2)) as f64
    } else {
        1.0
    };

    let mut out: Vec<TableCentrality> = schema
        .tables
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let share = if n > 2 { (between[i] / norm).min(1.0) } else { 0.0 };
            let rb = referenced_by[i].len();
            let rf = references[i].len();
            let role = if adj[i].is_empty() {
                Role::Isolated
            } else if rb >= HUB_MIN_REFERENCES {
                Role::Hub
            } else if share >= BRIDGE_MIN_SHARE {
                Role::Bridge
            } else {
                Role::Regular
            };
            let explanation = match role {
                Role::Isolated => "No foreign keys connect this table to the rest of the schema.".to_string(),
                _ => format!(
                    "Referenced by {rb} table{} and references {rf}. {}",
                    if rb == 1 { "" } else { "s" },
                    if share > 0.0 {
                        format!("About {:.0}% of the shortest relationship paths between other tables pass through it.", share * 100.0)
                    } else {
                        "No shortest relationship path between other tables passes through it.".to_string()
                    }
                ),
            };
            TableCentrality { table: t.stable_id.clone(), name: format!("{}.{}", t.namespace, t.name), referenced_by: rb, references: rf, path_share: share, role, explanation }
        })
        .collect();
    out.sort_by(|a, b| {
        b.referenced_by
            .cmp(&a.referenced_by)
            .then(
                b.path_share
                    .partial_cmp(&a.path_share)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.table.cmp(&b.table))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_support::*;

    fn star(center: &str, leaves: &[&str]) -> DatabaseSchema {
        let mut s = DatabaseSchema::empty(Engine::Postgres, "x");
        s.tables.push(table(center));
        for l in leaves {
            let mut t = table(l);
            t.foreign_keys.push(ForeignKey {
                stable_id: StableId::child(Engine::Postgres, "constraint", "public", l, "fk"),
                name: "fk".into(),
                namespace: "public".into(),
                source_table: tid(l),
                source_columns: vec!["c".into()],
                target_table: tid(center),
                target_columns: vec!["id".into()],
                on_delete: ReferentialAction::NoAction,
                on_update: ReferentialAction::NoAction,
                engine_specific_metadata: EngineMeta::new(),
            });
            s.tables.push(t);
        }
        s
    }

    #[test]
    fn star_center_is_a_hub_with_full_path_share() {
        let s = star("users", &["a", "b", "c", "d", "e"]);
        let c = centrality(&s);
        assert_eq!(c[0].name, "public.users");
        assert_eq!(c[0].role, Role::Hub);
        assert_eq!(c[0].referenced_by, 5);
        assert!(
            (c[0].path_share - 1.0).abs() < 1e-9,
            "every leaf-to-leaf path crosses the center"
        );
        assert!(c[0].explanation.contains("Referenced by 5 tables"));
        assert!(c
            .iter()
            .skip(1)
            .all(|x| x.role == Role::Regular && x.path_share == 0.0));
    }

    #[test]
    fn chain_middle_is_a_bridge() {
        let mut s = DatabaseSchema::empty(Engine::Postgres, "x");
        for n in ["a", "b", "c"] {
            s.tables.push(table(n));
        }
        for (from, to) in [("a", "b"), ("b", "c")] {
            s.tables
                .iter_mut()
                .find(|t| t.name == from)
                .unwrap()
                .foreign_keys
                .push(ForeignKey {
                    stable_id: StableId::child(
                        Engine::Postgres,
                        "constraint",
                        "public",
                        from,
                        "fk",
                    ),
                    name: "fk".into(),
                    namespace: "public".into(),
                    source_table: tid(from),
                    source_columns: vec!["c".into()],
                    target_table: tid(to),
                    target_columns: vec!["id".into()],
                    on_delete: ReferentialAction::NoAction,
                    on_update: ReferentialAction::NoAction,
                    engine_specific_metadata: EngineMeta::new(),
                });
        }
        let c = centrality(&s);
        let b = c.iter().find(|x| x.name == "public.b").unwrap();
        assert_eq!(b.role, Role::Bridge);
        assert!(
            (b.path_share - 1.0).abs() < 1e-9,
            "the only other pair (a, c) must pass through b"
        );
    }

    #[test]
    fn unconnected_tables_are_isolated_and_empty_schema_is_fine() {
        let c = centrality(&sample());
        assert_eq!(
            c.iter().find(|x| x.name == "public.lonely").unwrap().role,
            Role::Isolated
        );
        assert!(centrality(&DatabaseSchema::empty(Engine::Postgres, "x")).is_empty());
    }
}
