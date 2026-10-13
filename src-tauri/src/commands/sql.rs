use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Notify;

use crate::error::{AppError, AppResult};
use crate::importer::{self, ImportResult, SqlDialect};
use crate::plan::QueryPlan;
use crate::query::{self, QueryResult};
use crate::state::{Active, ActiveInfo, AppState, CachedSchema};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DialectArg {
    Postgres,
    Mysql,
    Sqlite,
    Generic,
}

impl From<DialectArg> for SqlDialect {
    fn from(d: DialectArg) -> Self {
        match d {
            DialectArg::Postgres => SqlDialect::Postgres,
            DialectArg::Mysql => SqlDialect::Mysql,
            DialectArg::Sqlite => SqlDialect::Sqlite,
            DialectArg::Generic => SqlDialect::Generic,
        }
    }
}

/// Scripts larger than this are refused rather than parsed (bounded memory and time).
pub const MAX_SCRIPT_BYTES: usize = 8 * 1024 * 1024;

fn check_size(sql: &str) -> AppResult<()> {
    if sql.len() > MAX_SCRIPT_BYTES {
        return Err(AppError::Validation(format!(
            "the script is {} MB; the limit is {} MB",
            sql.len() / (1024 * 1024),
            MAX_SCRIPT_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

/// Parses SQL without touching any database or workspace state.
#[tauri::command]
pub fn import_sql(sql: String, dialect: DialectArg) -> AppResult<ImportResult> {
    check_size(&sql)?;
    Ok(importer::import_sql(&sql, dialect.into()))
}

/// Opens the schema described by `sql` as an offline workspace.
#[tauri::command]
pub async fn open_import(
    state: State<'_, AppState>,
    sql: String,
    dialect: DialectArg,
    name: Option<String>,
) -> AppResult<ActiveInfo> {
    check_size(&sql)?;
    let result = importer::import_sql(&sql, dialect.into());
    if result.schema.tables.is_empty() && result.schema.views.is_empty() {
        return Err(AppError::Validation(
            "the script defines no tables or views to open".into(),
        ));
    }
    let cached = Arc::new(CachedSchema::new(result.schema, true));
    let layout_key = format!("imported:{}", cached.fingerprint);
    state
        .replace_active(Active {
            session: None,
            name: name.unwrap_or_else(|| "Imported SQL".into()),
            connection_id: None,
            cache: Some(cached),
            layout_key,
        })
        .await;
    state.info().await.ok_or(AppError::NotConnected)
}

#[tauri::command]
pub async fn run_query(
    state: State<'_, AppState>,
    id: String,
    sql: String,
    limit: Option<usize>,
    timeout_ms: Option<u64>,
) -> AppResult<QueryResult> {
    let session = state.session().await?;
    let cancel = Arc::new(Notify::new());
    state
        .running
        .lock()
        .await
        .insert(id.clone(), cancel.clone());
    let result = query::run_query(&session, &sql, limit, timeout_ms, cancel).await;
    state.running.lock().await.remove(&id);
    result
}

#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, id: String) -> AppResult<bool> {
    let handle = state.running.lock().await.get(&id).cloned();
    match handle {
        Some(n) => {
            n.notify_one();
            Ok(true)
        }
        None => Ok(false),
    }
}

#[tauri::command]
pub async fn explain_query(state: State<'_, AppState>, sql: String) -> AppResult<QueryPlan> {
    let session = state.session().await?;
    query::explain(&session, &sql).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub live: bool,
}

#[tauri::command]
pub async fn capabilities(state: State<'_, AppState>) -> AppResult<Capabilities> {
    Ok(Capabilities {
        live: state.session().await.is_ok(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_scripts_are_refused() {
        assert!(check_size("select 1").is_ok());
        let big = "x".repeat(MAX_SCRIPT_BYTES + 1);
        assert!(matches!(check_size(&big), Err(AppError::Validation(_))));
    }
}
