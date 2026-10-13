pub mod normalize;
pub mod queries;
pub mod raw;

use sqlx::PgPool;

use crate::error::{AppError, AppResult};
use crate::metadata::DatabaseSchema;
use raw::RawCatalog;

/// Reads the whole catalog with a handful of set-based queries (no per-table round trips).
pub async fn introspect(pool: &PgPool, include_system: bool) -> AppResult<DatabaseSchema> {
    let catalog = fetch_catalog(pool, include_system).await?;
    let schema = normalize::normalize(catalog);
    normalize::validate(&schema).map_err(AppError::Introspection)?;
    Ok(schema)
}

pub async fn fetch_catalog(pool: &PgPool, include_system: bool) -> AppResult<RawCatalog> {
    // One read-only snapshot-consistent transaction for the whole introspection.
    let mut tx = pool.begin().await.map_err(map)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(map)?;

    // pg_get_*def() output depends on search_path; pin it so definitions are always
    // schema-qualified and identical across sessions.
    sqlx::query("SET LOCAL search_path TO pg_catalog").execute(&mut *tx).await.map_err(map)?;

    macro_rules! rows {
        ($q:expr) => {
            sqlx::query_as($q).bind(include_system).fetch_all(&mut *tx).await.map_err(map)?
        };
    }

    let version: String = sqlx::query_scalar(queries::VERSION).fetch_one(&mut *tx).await.map_err(map)?;
    let database: String = sqlx::query_scalar(queries::DATABASE).fetch_one(&mut *tx).await.map_err(map)?;
    let catalog = RawCatalog {
        version,
        database,
        namespaces: rows!(queries::NAMESPACES),
        relations: rows!(queries::RELATIONS),
        columns: rows!(queries::COLUMNS),
        constraints: rows!(queries::CONSTRAINTS),
        indexes: rows!(queries::INDEXES),
        view_dependencies: rows!(queries::VIEW_DEPENDENCIES),
        triggers: rows!(queries::TRIGGERS),
        sequences: rows!(queries::SEQUENCES),
        routines: rows!(queries::ROUTINES),
    };
    tx.rollback().await.map_err(map)?;
    Ok(catalog)
}

fn map(e: sqlx::Error) -> AppError {
    AppError::Introspection(e.to_string())
}
