//! Raw rows as returned by `pg_catalog`. These never leave the Rust side:
//! the normalizer turns them into the engine independent `DatabaseSchema`.

use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct RawNamespace {
    pub name: String,
    pub is_system: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct RawRelation {
    pub namespace: String,
    pub name: String,
    /// `r` table, `p` partitioned table, `v` view, `m` materialized view
    pub kind: String,
    pub comment: Option<String>,
    pub estimated_rows: Option<i64>,
    pub size_bytes: Option<i64>,
    pub view_definition: Option<String>,
    pub is_partition: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct RawColumn {
    pub namespace: String,
    pub relation: String,
    pub name: String,
    pub ordinal: i32,
    pub native_type: String,
    pub not_null: bool,
    pub default: Option<String>,
    /// `a` always, `d` by default
    pub identity: Option<String>,
    /// `s` stored
    pub generated: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct RawConstraint {
    pub namespace: String,
    pub relation: String,
    pub name: String,
    /// `p` primary, `f` foreign, `u` unique, `c` check
    pub kind: String,
    pub columns: Vec<String>,
    pub ref_namespace: Option<String>,
    pub ref_relation: Option<String>,
    pub ref_columns: Option<Vec<String>>,
    pub on_update: Option<String>,
    pub on_delete: Option<String>,
    pub definition: String,
    pub deferrable: bool,
    pub validated: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct RawIndex {
    pub namespace: String,
    pub table: String,
    pub name: String,
    pub method: String,
    pub is_unique: bool,
    pub is_primary: bool,
    pub is_valid: bool,
    /// Key entries in order. Expression entries hold the expression text.
    pub key_entries: Vec<String>,
    /// Same length as `key_entries`: true when the entry is an expression.
    pub key_is_expr: Vec<bool>,
    pub included: Vec<String>,
    pub predicate: Option<String>,
    pub definition: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct RawViewDependency {
    pub view_namespace: String,
    pub view_name: String,
    pub source_namespace: String,
    pub source_name: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct RawTrigger {
    pub namespace: String,
    pub table: String,
    pub name: String,
    pub timing: String,
    pub events: Vec<String>,
    pub definition: String,
    pub function_namespace: String,
    pub function_name: String,
    pub function_args: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct RawSequence {
    pub namespace: String,
    pub name: String,
    pub data_type: String,
    pub owner_namespace: Option<String>,
    pub owner_table: Option<String>,
    pub owner_column: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct RawRoutine {
    pub namespace: String,
    pub name: String,
    pub is_procedure: bool,
    pub language: String,
    pub arguments: String,
    pub return_type: Option<String>,
    pub definition: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RawCatalog {
    pub version: String,
    pub database: String,
    pub namespaces: Vec<RawNamespace>,
    pub relations: Vec<RawRelation>,
    pub columns: Vec<RawColumn>,
    pub constraints: Vec<RawConstraint>,
    pub indexes: Vec<RawIndex>,
    pub view_dependencies: Vec<RawViewDependency>,
    pub triggers: Vec<RawTrigger>,
    pub sequences: Vec<RawSequence>,
    pub routines: Vec<RawRoutine>,
}
