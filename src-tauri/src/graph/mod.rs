//! Dependency graph over the normalized schema, built on petgraph.
//!
//! Edge direction: `dependent -> dependency` (a view points at the tables it reads, a
//! foreign key points from the referencing table to the referenced one). "Impact" of an
//! object is therefore everything that can reach it by following edges backwards.

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use serde::{Deserialize, Serialize};

use crate::metadata::{DatabaseSchema, DependencyKind, StableId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Table,
    View,
    MaterializedView,
    Routine,
    Trigger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: StableId,
    pub name: String,
    pub namespace: String,
    pub kind: ObjectKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from: StableId,
    pub to: StableId,
    pub kind: DependencyKind,
}

/// Serializable projection used by the UI. React only renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphView {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

pub struct SchemaGraph {
    graph: DiGraph<GraphNode, DependencyKind>,
    index: HashMap<StableId, NodeIndex>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reached {
    pub node: GraphNode,
    pub depth: usize,
    /// Edge kind of the first hop that led here.
    pub kind: DependencyKind,
    /// Node the traversal came from (parent in the BFS tree).
    pub via: StableId,
}

impl SchemaGraph {
    pub fn build(schema: &DatabaseSchema) -> Self {
        let mut graph = DiGraph::new();
        let mut index = HashMap::new();
        let mut add = |id: &StableId, name: &str, ns: &str, kind: ObjectKind| {
            let n = graph.add_node(GraphNode {
                id: id.clone(),
                name: name.into(),
                namespace: ns.into(),
                kind,
            });
            index.insert(id.clone(), n);
        };
        for t in &schema.tables {
            add(&t.stable_id, &t.name, &t.namespace, ObjectKind::Table);
        }
        for v in &schema.views {
            let kind = if v.materialized {
                ObjectKind::MaterializedView
            } else {
                ObjectKind::View
            };
            add(&v.stable_id, &v.name, &v.namespace, kind);
        }
        for r in &schema.routines {
            add(&r.stable_id, &r.name, &r.namespace, ObjectKind::Routine);
        }
        for t in &schema.triggers {
            add(&t.stable_id, &t.name, &t.namespace, ObjectKind::Trigger);
        }
        for d in &schema.dependencies {
            if let (Some(&a), Some(&b)) = (index.get(&d.from), index.get(&d.to)) {
                graph.add_edge(a, b, d.kind);
            }
        }
        Self { graph, index }
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn node(&self, id: &StableId) -> Option<&GraphNode> {
        self.index.get(id).map(|&i| &self.graph[i])
    }

    pub fn contains(&self, id: &StableId) -> bool {
        self.index.contains_key(id)
    }

    pub fn view(&self) -> GraphView {
        let mut nodes: Vec<GraphNode> = self.graph.node_weights().cloned().collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        let mut edges: Vec<GraphEdge> = self
            .graph
            .edge_indices()
            .filter_map(|e| {
                let (a, b) = self.graph.edge_endpoints(e)?;
                Some(GraphEdge {
                    from: self.graph[a].id.clone(),
                    to: self.graph[b].id.clone(),
                    kind: self.graph[e],
                })
            })
            .collect();
        edges.sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
        GraphView { nodes, edges }
    }

    /// Objects that depend on `id` (reverse reachability), breadth first, shortest depth wins.
    pub fn dependents(&self, id: &StableId, max_depth: Option<usize>) -> Vec<Reached> {
        self.bfs(id, Direction::Incoming, max_depth)
    }

    /// Objects `id` depends on (forward reachability).
    pub fn dependencies(&self, id: &StableId, max_depth: Option<usize>) -> Vec<Reached> {
        self.bfs(id, Direction::Outgoing, max_depth)
    }

    fn bfs(&self, id: &StableId, dir: Direction, max_depth: Option<usize>) -> Vec<Reached> {
        let Some(&start) = self.index.get(id) else {
            return vec![];
        };
        let mut seen: HashSet<NodeIndex> = HashSet::from([start]);
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::from([(start, 0)]);
        let mut out = vec![];
        while let Some((n, depth)) = queue.pop_front() {
            if max_depth.is_some_and(|m| depth >= m) {
                continue;
            }
            // Deterministic: visit neighbours in stable id order.
            let mut next: Vec<(NodeIndex, DependencyKind)> = self
                .graph
                .edges_directed(n, dir)
                .map(|e| {
                    use petgraph::visit::EdgeRef;
                    (
                        if dir == Direction::Incoming {
                            e.source()
                        } else {
                            e.target()
                        },
                        *e.weight(),
                    )
                })
                .collect();
            next.sort_by(|a, b| (&self.graph[a.0].id, a.1).cmp(&(&self.graph[b.0].id, b.1)));
            for (m, kind) in next {
                if seen.insert(m) {
                    out.push(Reached {
                        node: self.graph[m].clone(),
                        depth: depth + 1,
                        kind,
                        via: self.graph[n].id.clone(),
                    });
                    queue.push_back((m, depth + 1));
                }
            }
        }
        out
    }

    /// Everything within `hops` of `id`, ignoring edge direction (Focus Mode).
    pub fn neighborhood(&self, id: &StableId, hops: Option<usize>) -> Vec<StableId> {
        self.neighborhood_of(id, hops, None)
    }

    /// Like [`Self::neighborhood`], optionally following only the given edge kinds.
    pub fn neighborhood_of(
        &self,
        id: &StableId,
        hops: Option<usize>,
        kinds: Option<&[DependencyKind]>,
    ) -> Vec<StableId> {
        let Some(&start) = self.index.get(id) else {
            return vec![];
        };
        let mut seen: HashSet<NodeIndex> = HashSet::from([start]);
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::from([(start, 0)]);
        while let Some((n, d)) = queue.pop_front() {
            if hops.is_some_and(|h| d >= h) {
                continue;
            }
            for e in self
                .graph
                .edges(n)
                .chain(self.graph.edges_directed(n, Direction::Incoming))
            {
                use petgraph::visit::EdgeRef;
                if kinds.is_some_and(|k| !k.contains(e.weight())) {
                    continue;
                }
                let m = if e.source() == n {
                    e.target()
                } else {
                    e.source()
                };
                if seen.insert(m) {
                    queue.push_back((m, d + 1));
                }
            }
        }
        let mut ids: Vec<StableId> = seen.into_iter().map(|i| self.graph[i].id.clone()).collect();
        ids.sort();
        ids
    }

    /// Shortest relationship path, ignoring direction. Returns node ids from `a` to `b`.
    pub fn shortest_path(&self, a: &StableId, b: &StableId) -> Option<Vec<StableId>> {
        let (&s, &t) = (self.index.get(a)?, self.index.get(b)?);
        let mut prev: HashMap<NodeIndex, NodeIndex> = HashMap::new();
        let mut seen = HashSet::from([s]);
        let mut queue = VecDeque::from([s]);
        while let Some(n) = queue.pop_front() {
            if n == t {
                break;
            }
            let mut ns: Vec<NodeIndex> = self.graph.neighbors_undirected(n).collect();
            ns.sort_by(|x, y| self.graph[*x].id.cmp(&self.graph[*y].id));
            for m in ns {
                if seen.insert(m) {
                    prev.insert(m, n);
                    queue.push_back(m);
                }
            }
        }
        if !seen.contains(&t) {
            return None;
        }
        let mut path = vec![t];
        let mut cur = t;
        while cur != s {
            cur = prev[&cur];
            path.push(cur);
        }
        path.reverse();
        Some(path.into_iter().map(|i| self.graph[i].id.clone()).collect())
    }

    /// Simple paths between two objects, shortest first, bounded in length and count.
    pub fn alternative_paths(
        &self,
        a: &StableId,
        b: &StableId,
        max_len: usize,
        limit: usize,
    ) -> Vec<Vec<StableId>> {
        let (Some(&s), Some(&t)) = (self.index.get(a), self.index.get(b)) else {
            return vec![];
        };
        let mut found: Vec<Vec<NodeIndex>> = vec![];
        let mut stack = vec![s];
        self.dfs_paths(t, max_len, &mut stack, &mut found);
        found.sort_by(|x, y| {
            x.len().cmp(&y.len()).then_with(|| {
                let kx: Vec<_> = x.iter().map(|i| &self.graph[*i].id).collect();
                let ky: Vec<_> = y.iter().map(|i| &self.graph[*i].id).collect();
                kx.cmp(&ky)
            })
        });
        found.truncate(limit);
        found
            .into_iter()
            .map(|p| p.into_iter().map(|i| self.graph[i].id.clone()).collect())
            .collect()
    }

    fn dfs_paths(
        &self,
        target: NodeIndex,
        max_len: usize,
        stack: &mut Vec<NodeIndex>,
        out: &mut Vec<Vec<NodeIndex>>,
    ) {
        let cur = *stack.last().unwrap();
        if cur == target {
            out.push(stack.clone());
            return;
        }
        if stack.len() > max_len || out.len() > 500 {
            return;
        }
        let mut ns: Vec<NodeIndex> = self.graph.neighbors_undirected(cur).collect();
        ns.sort_by(|x, y| self.graph[*x].id.cmp(&self.graph[*y].id));
        ns.dedup();
        for m in ns {
            if !stack.contains(&m) {
                stack.push(m);
                self.dfs_paths(target, max_len, stack, out);
                stack.pop();
            }
        }
    }

    /// Strongly connected components with more than one member, or self loops.
    /// Restricted to foreign keys when `kinds` is given.
    pub fn cycles(&self, kinds: Option<&[DependencyKind]>) -> Vec<Vec<StableId>> {
        let filtered = self.graph.filter_map(
            |_, n| Some(n.clone()),
            |_, e| kinds.is_none_or(|k| k.contains(e)).then_some(*e),
        );
        let mut out: Vec<Vec<StableId>> = tarjan_scc(&filtered)
            .into_iter()
            .filter(|c| c.len() > 1 || filtered.contains_edge(c[0], c[0]))
            .map(|c| {
                let mut ids: Vec<StableId> =
                    c.into_iter().map(|i| filtered[i].id.clone()).collect();
                ids.sort();
                ids
            })
            .collect();
        out.sort();
        out
    }

    /// (in_degree, out_degree) per node: how many objects depend on it / it depends on.
    pub fn degree(&self, id: &StableId) -> Option<(usize, usize)> {
        let &i = self.index.get(id)?;
        Some((
            self.graph.edges_directed(i, Direction::Incoming).count(),
            self.graph.edges_directed(i, Direction::Outgoing).count(),
        ))
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::metadata::*;

    pub fn table(name: &str) -> Table {
        Table {
            stable_id: StableId::table(Engine::Postgres, "public", name),
            name: name.into(),
            namespace: "public".into(),
            comment: None,
            estimated_rows: None,
            size_bytes: None,
            columns: vec![],
            primary_key: None,
            foreign_keys: vec![],
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes: vec![],
            engine_specific_metadata: EngineMeta::new(),
        }
    }

    pub fn tid(name: &str) -> StableId {
        StableId::table(Engine::Postgres, "public", name)
    }

    pub fn vid(name: &str) -> StableId {
        StableId::object(Engine::Postgres, "view", "public", name)
    }

    pub fn view(name: &str) -> View {
        View {
            stable_id: vid(name),
            name: name.into(),
            namespace: "public".into(),
            materialized: false,
            definition: None,
            columns: vec![],
            comment: None,
            engine_specific_metadata: EngineMeta::new(),
        }
    }

    pub fn fk_dep(from: &str, to: &str) -> Dependency {
        Dependency {
            from: tid(from),
            to: tid(to),
            kind: DependencyKind::ForeignKey,
        }
    }

    /// users <- orders <- order_items ; view v_orders reads orders ; view v_top reads v_orders
    pub fn sample() -> DatabaseSchema {
        let mut s = DatabaseSchema::empty(Engine::Postgres, "app");
        s.tables = vec![
            table("users"),
            table("orders"),
            table("order_items"),
            table("lonely"),
        ];
        s.views = vec![view("v_orders"), view("v_top")];
        s.dependencies = vec![
            fk_dep("orders", "users"),
            fk_dep("order_items", "orders"),
            Dependency {
                from: vid("v_orders"),
                to: tid("orders"),
                kind: DependencyKind::DependsOn,
            },
            Dependency {
                from: vid("v_top"),
                to: vid("v_orders"),
                kind: DependencyKind::DependsOn,
            },
        ];
        s
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    #[test]
    fn builds_nodes_and_edges() {
        let g = SchemaGraph::build(&sample());
        assert_eq!(g.node_count(), 6);
        assert_eq!(g.edge_count(), 4);
    }

    #[test]
    fn dependents_are_transitive_with_depth() {
        let g = SchemaGraph::build(&sample());
        let d = g.dependents(&tid("users"), None);
        let by_name: HashMap<_, _> = d.iter().map(|r| (r.node.name.as_str(), r.depth)).collect();
        assert_eq!(by_name["orders"], 1);
        assert_eq!(by_name["order_items"], 2);
        assert_eq!(by_name["v_orders"], 2);
        assert_eq!(by_name["v_top"], 3);
        assert!(!by_name.contains_key("lonely"));
        assert_eq!(g.dependents(&tid("users"), Some(1)).len(), 1);
    }

    #[test]
    fn dependencies_walk_forward() {
        let g = SchemaGraph::build(&sample());
        let d = g.dependencies(&vid("v_top"), None);
        assert_eq!(
            d.iter().map(|r| r.node.name.as_str()).collect::<Vec<_>>(),
            ["v_orders", "orders", "users"]
        );
    }

    #[test]
    fn shortest_path_and_alternatives() {
        let g = SchemaGraph::build(&sample());
        let p = g.shortest_path(&tid("order_items"), &tid("users")).unwrap();
        assert_eq!(p.len(), 3);
        assert!(g.shortest_path(&tid("lonely"), &tid("users")).is_none());
        let alts = g.alternative_paths(&vid("v_top"), &tid("users"), 8, 5);
        assert_eq!(alts.len(), 1);
        assert_eq!(alts[0].len(), 4);
    }

    #[test]
    fn neighborhood_respects_hops() {
        let g = SchemaGraph::build(&sample());
        assert_eq!(g.neighborhood(&tid("orders"), Some(1)).len(), 4);
        assert_eq!(g.neighborhood(&tid("users"), Some(1)).len(), 2);
        assert_eq!(g.neighborhood(&tid("users"), None).len(), 5);
        assert_eq!(g.neighborhood(&tid("lonely"), None).len(), 1);
    }

    #[test]
    fn neighborhood_can_follow_only_foreign_keys() {
        let g = SchemaGraph::build(&sample());
        let fk = [DependencyKind::ForeignKey];
        let ids = g.neighborhood_of(&tid("orders"), None, Some(&fk));
        assert_eq!(ids.len(), 3, "users, orders, order_items; views excluded");
        assert!(!ids.contains(&vid("v_orders")));
    }

    #[test]
    fn detects_foreign_key_cycles() {
        let mut s = sample();
        s.dependencies.push(fk_dep("users", "order_items"));
        let g = SchemaGraph::build(&s);
        let cycles = g.cycles(Some(&[DependencyKind::ForeignKey]));
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
        assert!(SchemaGraph::build(&sample()).cycles(None).is_empty());
    }

    #[test]
    fn self_referencing_fk_is_a_cycle() {
        let mut s = sample();
        s.dependencies.push(fk_dep("users", "users"));
        let g = SchemaGraph::build(&s);
        assert_eq!(g.cycles(Some(&[DependencyKind::ForeignKey])).len(), 1);
    }

    #[test]
    fn degrees() {
        let g = SchemaGraph::build(&sample());
        assert_eq!(g.degree(&tid("orders")), Some((2, 1)));
        assert_eq!(g.degree(&tid("nope")), None);
    }
}
