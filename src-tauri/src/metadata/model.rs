//! Engine independent schema model. React only ever sees these types.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use super::ids::StableId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Postgres,
    Mysql,
    Sqlite,
    /// Schema recovered from imported SQL, not from a live connection.
    Generic,
}

impl Engine {
    pub fn slug(self) -> &'static str {
        match self {
            Engine::Postgres => "postgres",
            Engine::Mysql => "mysql",
            Engine::Sqlite => "sqlite",
            Engine::Generic => "generic",
        }
    }
}

/// Free-form, engine specific details that do not fit the common model.
pub type EngineMeta = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSchema {
    pub engine: Engine,
    pub database: String,
    pub server_version: Option<String>,
    pub namespaces: Vec<DatabaseNamespace>,
    pub tables: Vec<Table>,
    pub views: Vec<View>,
    pub triggers: Vec<Trigger>,
    pub routines: Vec<Routine>,
    pub sequences: Vec<Sequence>,
    pub dependencies: Vec<Dependency>,
    /// Statements/objects that could not be understood (SQL import, partial introspection).
    pub warnings: Vec<String>,
}

impl DatabaseSchema {
    pub fn empty(engine: Engine, database: impl Into<String>) -> Self {
        Self {
            engine,
            database: database.into(),
            server_version: None,
            namespaces: vec![],
            tables: vec![],
            views: vec![],
            triggers: vec![],
            routines: vec![],
            sequences: vec![],
            dependencies: vec![],
            warnings: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseNamespace {
    pub stable_id: StableId,
    pub name: String,
    pub is_system: bool,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub comment: Option<String>,
    pub estimated_rows: Option<i64>,
    pub size_bytes: Option<i64>,
    pub columns: Vec<Column>,
    pub primary_key: Option<PrimaryKey>,
    pub foreign_keys: Vec<ForeignKey>,
    pub unique_constraints: Vec<UniqueConstraint>,
    pub check_constraints: Vec<CheckConstraint>,
    pub indexes: Vec<Index>,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub ordinal: i32,
    /// Normalized, engine independent type name (`integer`, `text`, ...).
    pub data_type: String,
    /// Type exactly as the engine reports it (`int4`, `character varying(255)`).
    pub native_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub generated: Option<String>,
    pub identity: Option<String>,
    pub comment: Option<String>,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimaryKey {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub columns: Vec<String>,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferentialAction {
    NoAction,
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub source_table: StableId,
    pub source_columns: Vec<String>,
    pub target_table: StableId,
    pub target_columns: Vec<String>,
    pub on_delete: ReferentialAction,
    pub on_update: ReferentialAction,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UniqueConstraint {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub columns: Vec<String>,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckConstraint {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub expression: String,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub table: StableId,
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
    pub method: Option<String>,
    pub predicate: Option<String>,
    /// Expressions for expression indexes, in key order. Empty for plain column indexes.
    pub expressions: Vec<String>,
    pub included_columns: Vec<String>,
    pub definition: Option<String>,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub materialized: bool,
    pub definition: Option<String>,
    pub columns: Vec<Column>,
    pub comment: Option<String>,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub table: StableId,
    pub timing: String,
    pub events: Vec<String>,
    pub definition: Option<String>,
    /// Routine invoked by the trigger, when the engine exposes it.
    pub function: Option<StableId>,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutineKind {
    Function,
    Procedure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub kind: RoutineKind,
    pub language: Option<String>,
    pub arguments: String,
    pub return_type: Option<String>,
    pub definition: Option<String>,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sequence {
    pub stable_id: StableId,
    pub name: String,
    pub namespace: String,
    pub data_type: Option<String>,
    /// Column that owns the sequence (serial/identity), when known.
    pub owned_by: Option<StableId>,
    pub engine_specific_metadata: EngineMeta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DependencyKind {
    ForeignKey,
    DependsOn,
    DerivesFrom,
    TriggerDependency,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    /// The dependent object (e.g. the view).
    pub from: StableId,
    /// The object being depended upon (e.g. the table).
    pub to: StableId,
    pub kind: DependencyKind,
}
