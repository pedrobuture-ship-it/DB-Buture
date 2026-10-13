//! Impact analysis: which database objects can be affected by changing `object_id`.
//!
//! Only database objects are visible. Application code, ORMs, reports and any external
//! consumer are NOT analyzed, so a result never proves that a change is safe, and DB-BUTURE
//! never claims that external code will break.

use serde::Serialize;

use super::Evidence;
use crate::graph::{ObjectKind, SchemaGraph};
use crate::metadata::text::mentions_identifier;
use crate::metadata::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactLevel {
    None,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Reach {
    Direct,
    Transitive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactItem {
    pub id: StableId,
    pub name: String,
    pub namespace: String,
    pub kind: ObjectKind,
    pub reach: Reach,
    pub relation: DependencyKind,
    pub depth: usize,
    pub via: StableId,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactCounts {
    pub direct: usize,
    pub transitive: usize,
    pub foreign_keys: usize,
    pub tables: usize,
    pub views: usize,
    pub materialized_views: usize,
    pub triggers: usize,
    pub routines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactReport {
    pub target: StableId,
    pub target_name: String,
    pub target_kind: String,
    pub column: Option<String>,
    pub level: ImpactLevel,
    /// Plain-language rule used to derive `level`.
    pub level_basis: String,
    pub direct: Vec<ImpactItem>,
    pub transitive: Vec<ImpactItem>,
    /// Indexes that include the analyzed column (column targets only).
    pub affected_indexes: Vec<String>,
    pub counts: ImpactCounts,
    pub notes: Vec<String>,
}

pub fn level_for(total: usize) -> ImpactLevel {
    match total {
        0 => ImpactLevel::None,
        1..=3 => ImpactLevel::Low,
        4..=9 => ImpactLevel::Medium,
        _ => ImpactLevel::High,
    }
}

const LEVEL_BASIS: &str =
    "Potential impact by number of dependent database objects: 0 none, 1-3 low, 4-9 medium, 10+ high.";

pub fn analyze_impact(
    schema: &DatabaseSchema,
    graph: &SchemaGraph,
    object_id: &StableId,
) -> Option<ImpactReport> {
    let mut notes = vec![
        "Only database objects are analyzed. Application code, ORMs, reports and other external \
         consumers are not visible to DB-BUTURE."
            .to_string(),
    ];

    // Resolve the target: an object in the graph, or a column of a table/view.
    let (node_id, column) = if graph.contains(object_id) {
        (object_id.clone(), None)
    } else {
        let (t, c) = find_column(schema, object_id)?;
        (t, Some(c))
    };
    let node = graph.node(&node_id)?;

    let mut items: Vec<ImpactItem> = vec![];
    let mut affected_indexes = vec![];

    match &column {
        None => {
            for r in graph.dependents(&node_id, None) {
                items.push(item(r.depth, r.node, r.kind, r.via, Evidence::ObservedFact));
            }
        }
        Some(col) => {
            let table = schema.tables.iter().find(|t| t.stable_id == node_id);
            if let Some(t) = table {
                affected_indexes = t
                    .indexes
                    .iter()
                    .filter(|i| i.columns.contains(col) || i.included_columns.contains(col))
                    .map(|i| i.name.clone())
                    .collect();
            }
            // Direct dependents of the table, narrowed to the column where the catalog allows it.
            for r in graph.dependents(&node_id, Some(1)) {
                let narrowed = match r.kind {
                    DependencyKind::ForeignKey => {
                        let hits = schema
                            .tables
                            .iter()
                            .find(|t| t.stable_id == r.node.id)
                            .is_some_and(|t| {
                                t.foreign_keys.iter().any(|f| {
                                    f.target_table == node_id && f.target_columns.contains(col)
                                })
                            });
                        if !hits {
                            continue; // an FK to other columns of the table is not affected
                        }
                        Evidence::ObservedFact
                    }
                    _ => {
                        let def = definition_of(schema, &r.node.id);
                        match def {
                            Some(d) if mentions_identifier(&d, col) => Evidence::Heuristic,
                            _ => Evidence::Unknown,
                        }
                    }
                };
                items.push(item(1, r.node, r.kind, r.via, narrowed));
            }
            // Own outgoing FKs: changing the column affects the constraint itself.
            if let Some(t) = table {
                for fk in t
                    .foreign_keys
                    .iter()
                    .filter(|f| f.source_columns.contains(col))
                {
                    if let Some(target) = graph.node(&fk.target_table) {
                        items.push(item(
                            1,
                            target.clone(),
                            DependencyKind::ForeignKey,
                            node_id.clone(),
                            Evidence::ObservedFact,
                        ));
                    }
                }
            }
            notes.push(
                "Column level view: views, triggers and routines are matched on the table. \
                 Column usage inside their SQL is a heuristic (text match) or unknown."
                    .into(),
            );
        }
    }

    // Transitive: dependents of everything directly affected (excluding the target and direct set).
    let direct_ids: std::collections::HashSet<StableId> = items
        .iter()
        .filter(|i| i.depth == 1)
        .map(|i| i.id.clone())
        .collect();
    let mut seen = direct_ids.clone();
    seen.insert(node_id.clone());
    let mut transitive = vec![];
    for d in items
        .iter()
        .filter(|i| i.depth == 1 && i.relation != DependencyKind::TriggerDependency)
        .cloned()
        .collect::<Vec<_>>()
    {
        for r in graph.dependents(&d.id, None) {
            if seen.insert(r.node.id.clone()) {
                transitive.push(item(
                    r.depth + 1,
                    r.node,
                    r.kind,
                    r.via,
                    Evidence::ObservedFact,
                ));
            }
        }
    }
    if column.is_some() {
        for t in &mut transitive {
            t.evidence = Evidence::Heuristic;
        }
    }

    let mut direct: Vec<ImpactItem> = items.into_iter().filter(|i| i.depth == 1).collect();
    direct.dedup_by(|a, b| a.id == b.id && a.relation == b.relation);
    for i in &mut direct {
        i.reach = Reach::Direct;
    }
    for i in &mut transitive {
        i.reach = Reach::Transitive;
    }
    direct.sort_by(|a, b| a.id.cmp(&b.id));
    transitive.sort_by(|a, b| (a.depth, &a.id).cmp(&(b.depth, &b.id)));

    let mut counts = ImpactCounts {
        direct: direct.len(),
        transitive: transitive.len(),
        ..Default::default()
    };
    for i in direct.iter().chain(transitive.iter()) {
        if i.relation == DependencyKind::ForeignKey {
            counts.foreign_keys += 1;
        }
        match i.kind {
            ObjectKind::Table => counts.tables += 1,
            ObjectKind::View => counts.views += 1,
            ObjectKind::MaterializedView => counts.materialized_views += 1,
            ObjectKind::Trigger => counts.triggers += 1,
            ObjectKind::Routine => counts.routines += 1,
        }
    }

    if !schema.warnings.is_empty() {
        notes.push(
            "Introspection reported warnings: some objects may be missing from this analysis."
                .into(),
        );
    }

    Some(ImpactReport {
        target: object_id.clone(),
        target_name: match &column {
            Some(c) => format!("{}.{}", node.name, c),
            None => node.name.clone(),
        },
        target_kind: if column.is_some() {
            "column".into()
        } else {
            format!("{:?}", node.kind).to_lowercase()
        },
        column,
        level: level_for(counts.direct + counts.transitive),
        level_basis: LEVEL_BASIS.into(),
        direct,
        transitive,
        affected_indexes,
        counts,
        notes,
    })
}

fn item(
    depth: usize,
    node: crate::graph::GraphNode,
    relation: DependencyKind,
    via: StableId,
    evidence: Evidence,
) -> ImpactItem {
    ImpactItem {
        id: node.id,
        name: node.name,
        namespace: node.namespace,
        kind: node.kind,
        reach: if depth == 1 {
            Reach::Direct
        } else {
            Reach::Transitive
        },
        relation,
        depth,
        via,
        evidence,
    }
}

fn find_column(schema: &DatabaseSchema, id: &StableId) -> Option<(StableId, String)> {
    for t in &schema.tables {
        if let Some(c) = t.columns.iter().find(|c| &c.stable_id == id) {
            return Some((t.stable_id.clone(), c.name.clone()));
        }
    }
    for v in &schema.views {
        if let Some(c) = v.columns.iter().find(|c| &c.stable_id == id) {
            return Some((v.stable_id.clone(), c.name.clone()));
        }
    }
    None
}

fn definition_of(schema: &DatabaseSchema, id: &StableId) -> Option<String> {
    schema
        .views
        .iter()
        .find(|v| &v.stable_id == id)
        .and_then(|v| v.definition.clone())
        .or_else(|| {
            schema
                .routines
                .iter()
                .find(|r| &r.stable_id == id)
                .and_then(|r| r.definition.clone())
        })
        .or_else(|| {
            schema
                .triggers
                .iter()
                .find(|t| &t.stable_id == id)
                .and_then(|t| t.definition.clone())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_support::*;

    #[test]
    fn table_impact_separates_direct_and_transitive() {
        let s = sample();
        let g = SchemaGraph::build(&s);
        let r = analyze_impact(&s, &g, &tid("users")).unwrap();
        assert_eq!(r.counts.direct, 1);
        assert_eq!(r.counts.transitive, 3);
        assert_eq!(r.direct[0].name, "orders");
        assert!(r.direct.iter().all(|i| i.reach == Reach::Direct));
        assert!(r.transitive.iter().all(|i| i.reach == Reach::Transitive));
        assert_eq!(r.level, ImpactLevel::Medium);
        assert_eq!(r.counts.views, 2);
        assert!(r.notes[0].contains("Application code"));
    }

    #[test]
    fn isolated_object_has_no_impact() {
        let s = sample();
        let g = SchemaGraph::build(&s);
        let r = analyze_impact(&s, &g, &tid("lonely")).unwrap();
        assert_eq!(r.level, ImpactLevel::None);
        assert!(r.direct.is_empty() && r.transitive.is_empty());
    }

    #[test]
    fn unknown_object_returns_none() {
        let s = sample();
        let g = SchemaGraph::build(&s);
        assert!(analyze_impact(&s, &g, &tid("ghost")).is_none());
    }

    #[test]
    fn levels() {
        assert_eq!(level_for(0), ImpactLevel::None);
        assert_eq!(level_for(3), ImpactLevel::Low);
        assert_eq!(level_for(9), ImpactLevel::Medium);
        assert_eq!(level_for(10), ImpactLevel::High);
    }

    #[test]
    fn column_impact_only_counts_fks_that_reference_the_column() {
        let mut s = sample();
        for name in ["users", "orders"] {
            let t = s.tables.iter_mut().find(|t| t.name == name).unwrap();
            for c in ["id", "email"] {
                t.columns.push(Column {
                    stable_id: StableId::child(Engine::Postgres, "column", "public", name, c),
                    name: c.into(),
                    namespace: "public".into(),
                    ordinal: 1,
                    data_type: "text".into(),
                    native_type: "text".into(),
                    nullable: false,
                    default: None,
                    generated: None,
                    identity: None,
                    comment: None,
                    engine_specific_metadata: EngineMeta::new(),
                });
            }
        }
        s.tables
            .iter_mut()
            .find(|t| t.name == "orders")
            .unwrap()
            .foreign_keys
            .push(ForeignKey {
                stable_id: StableId::child(
                    Engine::Postgres,
                    "constraint",
                    "public",
                    "orders",
                    "fk",
                ),
                name: "fk".into(),
                namespace: "public".into(),
                source_table: tid("orders"),
                source_columns: vec!["id".into()],
                target_table: tid("users"),
                target_columns: vec!["id".into()],
                on_delete: ReferentialAction::NoAction,
                on_update: ReferentialAction::NoAction,
                engine_specific_metadata: EngineMeta::new(),
            });
        let g = SchemaGraph::build(&s);
        let id_col = StableId::child(Engine::Postgres, "column", "public", "users", "id");
        let email_col = StableId::child(Engine::Postgres, "column", "public", "users", "email");
        assert_eq!(analyze_impact(&s, &g, &id_col).unwrap().counts.direct, 1);
        assert_eq!(analyze_impact(&s, &g, &email_col).unwrap().counts.direct, 0);
        let r = analyze_impact(&s, &g, &id_col).unwrap();
        assert_eq!(r.target_name, "users.id");
        assert_eq!(r.target_kind, "column");
        assert!(r
            .transitive
            .iter()
            .all(|i| i.evidence == Evidence::Heuristic));
    }
}
