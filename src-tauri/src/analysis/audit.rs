//! Audit rule engine and the explainable health score.
//!
//! Every rule declares whether it reports an observed fact or a heuristic, and every score
//! penalty points back to the rule and object that caused it.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use super::indexes::{fk_without_index, redundant_indexes};
use super::{Evidence, Severity};
use crate::graph::SchemaGraph;
use crate::metadata::*;

pub const WIDE_TABLE_COLUMNS: usize = 50;
pub const HUB_DEPENDENTS: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Integrity,
    Performance,
    Maintainability,
    Complexity,
    Documentation,
}

pub const CATEGORIES: [Category; 5] = [
    Category::Integrity,
    Category::Performance,
    Category::Maintainability,
    Category::Complexity,
    Category::Documentation,
];

pub struct AuditContext<'a> {
    pub schema: &'a DatabaseSchema,
    pub graph: &'a SchemaGraph,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub rule_id: &'static str,
    pub severity: Severity,
    pub category: Category,
    pub evidence: Evidence,
    pub object: StableId,
    pub object_name: String,
    pub message: String,
}

pub struct Rule {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub severity: Severity,
    pub category: Category,
    /// Whether findings are catalog facts or heuristics.
    pub evidence: Evidence,
    pub evaluate: fn(&AuditContext) -> Vec<(StableId, String, String)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub severity: Severity,
    pub category: Category,
    pub evidence: Evidence,
}

pub fn default_rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "table-without-pk",
            title: "Table without primary key",
            description: "Tables without a primary key cannot be reliably identified row by row and are hard to replicate.",
            severity: Severity::High,
            category: Category::Integrity,
            evidence: Evidence::ObservedFact,
            evaluate: |c| {
                c.schema.tables.iter()
                    .filter(|t| t.primary_key.is_none())
                    .map(|t| (t.stable_id.clone(), t.name.clone(), format!("{} has no primary key", t.name)))
                    .collect()
            },
        },
        Rule {
            id: "fk-without-index",
            title: "Foreign key without supporting index",
            description: "No index starts with the foreign key columns. Joins and cascades on the parent may scan the child table.",
            severity: Severity::Medium,
            category: Category::Performance,
            evidence: Evidence::ObservedFact,
            evaluate: |c| {
                fk_without_index(c.schema).into_iter()
                    .map(|g| (g.foreign_key, g.table_name.clone(),
                        format!("{}.{} ({}) has no supporting index", g.table_name, g.foreign_key_name, g.columns.join(", "))))
                    .collect()
            },
        },
        Rule {
            id: "duplicate-index",
            title: "Duplicate index",
            description: "Two indexes with an identical definition; one only costs write performance and space.",
            severity: Severity::Medium,
            category: Category::Performance,
            evidence: Evidence::ObservedFact,
            evaluate: |c| {
                redundant_indexes(c.schema).into_iter()
                    .filter(|r| r.kind == super::indexes::RedundancyKind::Duplicate)
                    .map(|r| (r.index, r.index_name.clone(), r.explanation))
                    .collect()
            },
        },
        Rule {
            id: "overlapping-index",
            title: "Possibly redundant index",
            description: "The index columns are a leading prefix of another index. It may be redundant, but ordering, covering and usage statistics decide.",
            severity: Severity::Low,
            category: Category::Performance,
            evidence: Evidence::Heuristic,
            evaluate: |c| {
                redundant_indexes(c.schema).into_iter()
                    .filter(|r| r.kind == super::indexes::RedundancyKind::Overlapping)
                    .map(|r| (r.index, r.index_name.clone(), r.explanation))
                    .collect()
            },
        },
        Rule {
            id: "wide-table",
            title: "Very wide table",
            description: "Tables with a very large number of columns are often hard to evolve and may mix responsibilities.",
            severity: Severity::Low,
            category: Category::Complexity,
            evidence: Evidence::Heuristic,
            evaluate: |c| {
                c.schema.tables.iter()
                    .filter(|t| t.columns.len() >= WIDE_TABLE_COLUMNS)
                    .map(|t| (t.stable_id.clone(), t.name.clone(), format!("{} has {} columns (threshold {})", t.name, t.columns.len(), WIDE_TABLE_COLUMNS)))
                    .collect()
            },
        },
        Rule {
            id: "circular-fk",
            title: "Circular foreign key dependency",
            description: "Tables reference each other through foreign keys, which complicates loading, deleting and migrating data.",
            severity: Severity::Medium,
            category: Category::Integrity,
            evidence: Evidence::ObservedFact,
            evaluate: |c| {
                c.graph.cycles(Some(&[DependencyKind::ForeignKey])).into_iter()
                    .filter(|cycle| cycle.len() > 1) // self references are a normal tree/hierarchy pattern
                    .map(|cycle| {
                        let names: Vec<String> = cycle.iter().filter_map(|id| c.graph.node(id)).map(|n| n.name.clone()).collect();
                        (cycle[0].clone(), names[0].clone(), format!("cycle: {}", names.join(" -> ")))
                    })
                    .collect()
            },
        },
        Rule {
            id: "naming-inconsistency",
            title: "Naming style inconsistency",
            description: "Names use a different case style than the rest of the schema. This is a convention heuristic.",
            severity: Severity::Low,
            category: Category::Maintainability,
            evidence: Evidence::Heuristic,
            evaluate: naming_inconsistency,
        },
        Rule {
            id: "invalid-index",
            title: "Invalid index",
            description: "The engine marks this index as invalid (for example after a failed concurrent build). It is not used by queries but still costs writes.",
            severity: Severity::Medium,
            category: Category::Performance,
            evidence: Evidence::ObservedFact,
            evaluate: |c| {
                c.schema.tables.iter().flat_map(|t| &t.indexes)
                    .filter(|i| i.engine_specific_metadata.get("valid").and_then(|v| v.as_bool()) == Some(false))
                    .map(|i| (i.stable_id.clone(), i.name.clone(), format!("index {} is marked invalid", i.name)))
                    .collect()
            },
        },
        Rule {
            id: "view-without-dependencies",
            title: "View with no resolved dependencies",
            description: "The catalog recorded no tables or views behind this view. Its lineage could not be resolved.",
            severity: Severity::Low,
            category: Category::Maintainability,
            evidence: Evidence::ObservedFact,
            evaluate: |c| {
                c.schema.views.iter()
                    .filter(|v| !c.schema.dependencies.iter().any(|d| d.from == v.stable_id))
                    .map(|v| (v.stable_id.clone(), v.name.clone(), format!("{} has no recorded dependencies", v.name)))
                    .collect()
            },
        },
        Rule {
            id: "hub-table",
            title: "Highly depended-upon object",
            description: "Many objects depend on this one, so changing it has a wide blast radius inside the database.",
            severity: Severity::Info,
            category: Category::Complexity,
            evidence: Evidence::ObservedFact,
            evaluate: |c| {
                c.schema.tables.iter()
                    .filter_map(|t| {
                        let n = c.graph.dependents(&t.stable_id, None).len();
                        (n >= HUB_DEPENDENTS).then(|| (t.stable_id.clone(), t.name.clone(), format!("{} has {} direct and transitive dependents", t.name, n)))
                    })
                    .collect()
            },
        },
        Rule {
            id: "table-without-comment",
            title: "Undocumented table",
            description: "The table has no comment in the catalog.",
            severity: Severity::Info,
            category: Category::Documentation,
            evidence: Evidence::ObservedFact,
            evaluate: |c| {
                c.schema.tables.iter()
                    .filter(|t| t.comment.as_deref().is_none_or(|s| s.trim().is_empty()))
                    .map(|t| (t.stable_id.clone(), t.name.clone(), format!("{} has no comment", t.name)))
                    .collect()
            },
        },
    ]
}

pub fn rule_catalog() -> Vec<RuleInfo> {
    default_rules()
        .iter()
        .map(|r| RuleInfo {
            id: r.id,
            title: r.title,
            description: r.description,
            severity: r.severity,
            category: r.category,
            evidence: r.evidence,
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Snake,
    Camel,
    Pascal,
    Kebab,
    Other,
}

fn style_of(name: &str) -> Style {
    let has_upper = name.chars().any(|c| c.is_uppercase());
    let has_lower = name.chars().any(|c| c.is_lowercase());
    if name.contains('-') {
        Style::Kebab
    } else if !has_upper {
        Style::Snake // snake_case or a single lowercase word
    } else if has_lower
        && name.chars().next().is_some_and(|c| c.is_uppercase())
        && !name.contains('_')
    {
        Style::Pascal
    } else if has_lower && !name.contains('_') {
        Style::Camel
    } else {
        Style::Other
    }
}

fn naming_inconsistency(c: &AuditContext) -> Vec<(StableId, String, String)> {
    let mut counts: HashMap<std::mem::Discriminant<Style>, usize> = HashMap::new();
    let names: Vec<(&StableId, &str)> = c
        .schema
        .tables
        .iter()
        .map(|t| (&t.stable_id, t.name.as_str()))
        .collect();
    for (_, n) in &names {
        *counts
            .entry(std::mem::discriminant(&style_of(n)))
            .or_default() += 1;
    }
    let Some((&dom, &dom_n)) = counts.iter().max_by_key(|(_, n)| **n) else {
        return vec![];
    };
    // Only meaningful when one style clearly dominates.
    if names.len() < 4 || dom_n * 10 < names.len() * 7 {
        return vec![];
    }
    names
        .iter()
        .filter(|(_, n)| std::mem::discriminant(&style_of(n)) != dom)
        .map(|(id, n)| {
            (
                (*id).clone(),
                n.to_string(),
                format!(
                    "{n} does not follow the dominant naming style ({dom_n} of {} tables)",
                    names.len()
                ),
            )
        })
        .collect()
}

pub fn run_audit(schema: &DatabaseSchema, graph: &SchemaGraph) -> Vec<Finding> {
    let ctx = AuditContext { schema, graph };
    let mut out = vec![];
    for rule in default_rules() {
        for (object, name, message) in (rule.evaluate)(&ctx) {
            out.push(Finding {
                rule_id: rule.id,
                severity: rule.severity,
                category: rule.category,
                evidence: rule.evidence,
                object,
                object_name: name,
                message,
            });
        }
    }
    out.sort_by(|a, b| {
        (std::cmp::Reverse(a.severity), a.rule_id, &a.object).cmp(&(
            std::cmp::Reverse(b.severity),
            b.rule_id,
            &b.object,
        ))
    });
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Penalty {
    pub rule_id: &'static str,
    pub object: StableId,
    pub object_name: String,
    pub points: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryScore {
    pub category: Category,
    pub score: f64,
    pub penalty_total: f64,
    pub penalties: Vec<Penalty>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    /// None when there is nothing to score (no tables).
    pub overall: Option<f64>,
    pub categories: Vec<CategoryScore>,
    pub budget: f64,
    pub method: String,
}

pub fn points(sev: Severity) -> f64 {
    match sev {
        Severity::High => 10.0,
        Severity::Medium => 5.0,
        Severity::Low => 2.0,
        Severity::Info => 0.0,
    }
}

pub fn health(schema: &DatabaseSchema, findings: &[Finding]) -> HealthReport {
    let tables = schema.tables.len();
    let budget = (tables as f64 * 3.0).max(20.0);
    let method = format!(
        "Each finding costs points by severity (high 10, medium 5, low 2, info 0). A category score is \
         100 minus the share of its penalties against a budget of max(20, 3 x tables) = {budget:.0} points. \
         Overall is the mean of the five categories. Heuristic findings are included and labelled."
    );
    let mut by_cat: BTreeMap<Category, Vec<Penalty>> = BTreeMap::new();
    for c in CATEGORIES {
        by_cat.insert(c, vec![]);
    }
    for f in findings {
        let p = points(f.severity);
        if p > 0.0 {
            by_cat.entry(f.category).or_default().push(Penalty {
                rule_id: f.rule_id,
                object: f.object.clone(),
                object_name: f.object_name.clone(),
                points: p,
            });
        }
    }
    let categories: Vec<CategoryScore> = by_cat
        .into_iter()
        .map(|(category, penalties)| {
            let total: f64 = penalties.iter().map(|p| p.points).sum();
            let score = (100.0 * (1.0 - (total / budget).min(1.0)) * 10.0).round() / 10.0;
            CategoryScore {
                category,
                score,
                penalty_total: total,
                penalties,
            }
        })
        .collect();
    let overall = (tables > 0).then(|| {
        let mean = categories.iter().map(|c| c.score).sum::<f64>() / categories.len() as f64;
        (mean * 10.0).round() / 10.0
    });
    HealthReport {
        overall,
        categories,
        budget,
        method,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::test_support::*;

    fn commented(mut s: DatabaseSchema) -> DatabaseSchema {
        for t in &mut s.tables {
            t.comment = Some("doc".into());
            t.primary_key = Some(PrimaryKey {
                stable_id: StableId::child(Engine::Postgres, "constraint", "public", &t.name, "pk"),
                name: "pk".into(),
                namespace: "public".into(),
                columns: vec!["id".into()],
                engine_specific_metadata: EngineMeta::new(),
            });
        }
        s
    }

    #[test]
    fn flags_tables_without_pk_as_fact() {
        let s = sample();
        let g = SchemaGraph::build(&s);
        let f = run_audit(&s, &g);
        let pk: Vec<_> = f
            .iter()
            .filter(|f| f.rule_id == "table-without-pk")
            .collect();
        assert_eq!(pk.len(), 4);
        assert!(pk
            .iter()
            .all(|f| f.evidence == Evidence::ObservedFact && f.severity == Severity::High));
    }

    #[test]
    fn clean_schema_scores_100_and_findings_have_no_penalties() {
        let s = commented(sample());
        let g = SchemaGraph::build(&s);
        let f = run_audit(&s, &g);
        assert!(f.iter().all(|f| points(f.severity) == 0.0), "{f:?}");
        let h = health(&s, &f);
        assert_eq!(h.overall, Some(100.0));
        assert_eq!(h.categories.len(), 5);
    }

    #[test]
    fn every_penalty_is_traceable_to_a_rule() {
        let s = sample();
        let g = SchemaGraph::build(&s);
        let f = run_audit(&s, &g);
        let h = health(&s, &f);
        let integrity = h
            .categories
            .iter()
            .find(|c| c.category == Category::Integrity)
            .unwrap();
        assert_eq!(integrity.penalties.len(), 4);
        assert!(integrity
            .penalties
            .iter()
            .all(|p| p.rule_id == "table-without-pk" && p.points == 10.0));
        assert_eq!(integrity.penalty_total, 40.0);
        assert!(integrity.score < 100.0);
        let rules: Vec<&str> = rule_catalog().iter().map(|r| r.id).collect();
        assert!(h
            .categories
            .iter()
            .flat_map(|c| &c.penalties)
            .all(|p| rules.contains(&p.rule_id)));
    }

    #[test]
    fn empty_schema_has_no_score() {
        let s = DatabaseSchema::empty(Engine::Postgres, "x");
        let g = SchemaGraph::build(&s);
        assert_eq!(health(&s, &run_audit(&s, &g)).overall, None);
    }

    #[test]
    fn circular_fk_reported_but_self_reference_is_not() {
        let mut s = commented(sample());
        s.dependencies.push(fk_dep("users", "users"));
        let g = SchemaGraph::build(&s);
        assert!(!run_audit(&s, &g).iter().any(|f| f.rule_id == "circular-fk"));
        s.dependencies.push(fk_dep("users", "order_items"));
        let g = SchemaGraph::build(&s);
        assert!(run_audit(&s, &g).iter().any(|f| f.rule_id == "circular-fk"));
    }

    #[test]
    fn naming_inconsistency_needs_a_clear_dominant_style() {
        let mut s = commented(sample());
        s.tables.push(commented_table("UserProfiles"));
        let g = SchemaGraph::build(&s);
        let f = run_audit(&s, &g);
        let n: Vec<_> = f
            .iter()
            .filter(|f| f.rule_id == "naming-inconsistency")
            .collect();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].object_name, "UserProfiles");
        assert_eq!(n[0].evidence, Evidence::Heuristic);
    }

    fn commented_table(name: &str) -> Table {
        commented(DatabaseSchema {
            tables: vec![table(name)],
            ..DatabaseSchema::empty(Engine::Postgres, "x")
        })
        .tables
        .remove(0)
    }

    #[test]
    fn rule_catalog_separates_facts_from_heuristics() {
        let c = rule_catalog();
        assert!(c.iter().any(|r| r.evidence == Evidence::ObservedFact));
        assert!(c.iter().any(|r| r.evidence == Evidence::Heuristic));
    }
}
