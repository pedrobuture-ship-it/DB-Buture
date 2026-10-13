//! Analysis engines. Everything here is pure: it reads the normalized model only.

pub mod audit;
pub mod centrality;
pub mod diff;
pub mod impact;
pub mod indexes;
pub mod lineage;
pub mod migration;
pub mod snapshot;

use serde::{Deserialize, Serialize};

/// How much a statement can be trusted. The UI must never present a heuristic as a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    /// Read directly from the catalog.
    ObservedFact,
    /// Inferred; may be wrong.
    Heuristic,
    /// Generated advice, never executed.
    Suggestion,
    /// The tool cannot know.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}
