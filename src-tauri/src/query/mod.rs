//! Read-only query execution for the SQL editor.
//!
//! Safety is layered: the SQL guard rejects mutations, the session itself is read-only
//! (`default_transaction_read_only`, `SET SESSION TRANSACTION READ ONLY`, read-only SQLite file),
//! statements time out, results are capped, and running statements can be cancelled.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{AssertSqlSafe, Column, Executor, Row, TypeInfo, ValueRef};
use tokio::sync::Notify;

use crate::database::session::{Pool, Session};
use crate::error::{AppError, AppResult};
use crate::metadata::Engine;
use crate::plan::{self, QueryPlan};
use crate::security::sql_guard::check_read_only;

pub const DEFAULT_LIMIT: usize = 500;
pub const MAX_LIMIT: usize = 10_000;
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultColumn {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: usize,
    /// More rows existed than `limit`.
    pub truncated: bool,
    pub limit: usize,
    pub elapsed_ms: u64,
}

/// Converts the text form of a value into JSON using the column type name.
pub fn to_json(type_name: &str, text: Option<String>) -> Value {
    let Some(t) = text else { return Value::Null };
    let ty = type_name.to_uppercase();
    match ty.as_str() {
        "INT2" | "INT4" | "INT8" | "OID" | "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "INTEGER" | "BIGINT"
        | "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "MEDIUMINT UNSIGNED" | "INT UNSIGNED" | "BIGINT UNSIGNED" => {
            t.parse::<i64>().map(Value::from).or_else(|_| t.parse::<u64>().map(Value::from)).unwrap_or(Value::String(t))
        }
        "FLOAT4" | "FLOAT8" | "FLOAT" | "DOUBLE" | "REAL" => match t.parse::<f64>() {
            Ok(f) if f.is_finite() => json!(f),
            _ => Value::String(t),
        },
        "BOOL" | "BOOLEAN" => match t.as_str() {
            "t" | "true" | "1" => Value::Bool(true),
            "f" | "false" | "0" => Value::Bool(false),
            _ => Value::String(t),
        },
        "JSON" | "JSONB" => serde_json::from_str(&t).unwrap_or(Value::String(t)),
        // NUMERIC / DECIMAL stay strings: converting to f64 would silently lose precision.
        _ => Value::String(t),
    }
}

fn clamp_limit(limit: Option<usize>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Runs a read-only statement. `cancel` interrupts it from another task.
pub async fn run_query(
    session: &Session,
    sql: &str,
    limit: Option<usize>,
    timeout_ms: Option<u64>,
    cancel: Arc<Notify>,
) -> AppResult<QueryResult> {
    check_read_only(sql, session.config.engine)?;
    let limit = clamp_limit(limit);
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS).clamp(100, 600_000));
    let start = Instant::now();

    // Only plain queries can be wrapped to cap the result; SHOW/EXPLAIN statements are small.
    let body = sql.trim().trim_end_matches(';').trim();
    let is_query = {
        use sqlparser::dialect::GenericDialect;
        sqlparser::parser::Parser::parse_sql(&GenericDialect {}, body)
            .map(|s| matches!(s.first(), Some(sqlparser::ast::Statement::Query(_))))
            .unwrap_or(true)
    };
    let wrapped = if is_query { format!("SELECT * FROM ({body}) AS buture_result LIMIT {}", limit + 1) } else { body.to_string() };

    let exec = execute(session, wrapped, limit);
    tokio::pin!(exec);
    let mut result = tokio::select! {
        r = &mut exec => r,
        _ = tokio::time::sleep(timeout) => Err(AppError::Timeout(timeout.as_millis() as u64)),
        _ = cancel.notified() => Err(AppError::Cancelled),
    };
    if matches!(result, Err(AppError::Timeout(_) | AppError::Cancelled)) {
        session.cancel_running().await;
    }
    if let Ok(r) = &mut result {
        r.elapsed_ms = start.elapsed().as_millis() as u64;
    }
    result
}

async fn execute(session: &Session, sql: String, limit: usize) -> AppResult<QueryResult> {
    match &session.pool {
        Pool::Postgres(p) => {
            let mut conn = p.acquire().await.map_err(map)?;
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *conn).await.map_err(map)?;
            session.set_running(pid as i64);
            let rows = sqlx::raw_sql(AssertSqlSafe(sql.clone())).fetch_all(&mut *conn).await;
            session.clear_running();
            let rows = rows.map_err(map)?;
            let cols = rows.first().map(|r| r.columns().iter().map(|c| (c.name().to_string(), c.type_info().name().to_string())).collect()).unwrap_or_default();
            let cols: Vec<(String, String)> = if rows.is_empty() { describe_pg(&mut conn, &sql).await } else { cols };
            let data = rows
                .iter()
                .take(limit)
                .map(|r| (0..r.columns().len()).map(|i| to_json(r.columns()[i].type_info().name(), r.try_get_unchecked::<Option<String>, _>(i).ok().flatten())).collect())
                .collect::<Vec<Vec<Value>>>();
            Ok(finish(cols, data, rows.len() > limit, limit))
        }
        Pool::Mysql(p) => {
            let mut conn = p.acquire().await.map_err(map)?;
            let id: i64 = sqlx::query_scalar("SELECT CAST(CONNECTION_ID() AS SIGNED)").fetch_one(&mut *conn).await.map_err(map)?;
            session.set_running(id);
            let body_for_describe = sql.clone();
            let rows = sqlx::raw_sql(AssertSqlSafe(sql)).fetch_all(&mut *conn).await;
            session.clear_running();
            let rows = rows.map_err(map)?;
            let cols: Vec<(String, String)> = match rows.first() {
                Some(r) => r.columns().iter().map(|c| (c.name().to_string(), c.type_info().name().to_string())).collect(),
                None => describe_mysql(&mut conn, &body_for_describe).await,
            };
            let data = rows
                .iter()
                .take(limit)
                .map(|r| (0..r.columns().len()).map(|i| to_json(r.columns()[i].type_info().name(), mysql_text(r, i))).collect())
                .collect::<Vec<Vec<Value>>>();
            Ok(finish(cols, data, rows.len() > limit, limit))
        }
        Pool::Sqlite(p) => {
            let rows = sqlx::raw_sql(AssertSqlSafe(sql.clone())).fetch_all(p).await.map_err(map)?;
            let cols: Vec<(String, String)> = match rows.first() {
                Some(r) => r.columns().iter().map(|c| (c.name().to_string(), c.type_info().name().to_string())).collect(),
                None => describe_sqlite(p, &sql).await,
            };
            let data = rows.iter().take(limit).map(|r| (0..r.columns().len()).map(|i| sqlite_value(r, i)).collect()).collect::<Vec<Vec<Value>>>();
            Ok(finish(cols, data, rows.len() > limit, limit))
        }
    }
}

fn finish(cols: Vec<(String, String)>, rows: Vec<Vec<Value>>, truncated: bool, limit: usize) -> QueryResult {
    QueryResult {
        columns: cols.into_iter().map(|(name, type_name)| ResultColumn { name, type_name }).collect(),
        row_count: rows.len(),
        rows,
        truncated,
        limit,
        elapsed_ms: 0,
    }
}

macro_rules! describe_fn {
    ($name:ident, $conn:ty) => {
        /// Column names for a statement without running it (used when a result has no rows).
        async fn $name(conn: $conn, sql: &str) -> Vec<(String, String)> {
            match Executor::describe(conn, sqlx::SqlSafeStr::into_sql_str(AssertSqlSafe(sql.to_string()))).await {
                Ok(d) => d.columns().iter().map(|c| (c.name().to_string(), c.type_info().name().to_string())).collect(),
                Err(_) => vec![],
            }
        }
    };
}
describe_fn!(describe_pg, &mut sqlx::PgConnection);
describe_fn!(describe_mysql, &mut sqlx::MySqlConnection);
describe_fn!(describe_sqlite, &sqlx::SqlitePool);

fn mysql_text(r: &sqlx::mysql::MySqlRow, i: usize) -> Option<String> {
    let raw = r.try_get_raw(i).ok()?;
    if raw.is_null() {
        return None;
    }
    match r.try_get_unchecked::<Option<Vec<u8>>, _>(i).ok().flatten() {
        Some(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
        None => None,
    }
}

fn sqlite_value(r: &sqlx::sqlite::SqliteRow, i: usize) -> Value {
    let Ok(raw) = r.try_get_raw(i) else { return Value::Null };
    if raw.is_null() {
        return Value::Null;
    }
    match raw.type_info().name() {
        "INTEGER" | "INT8" | "BOOLEAN" => r.try_get_unchecked::<i64, _>(i).map(Value::from).unwrap_or(Value::Null),
        "REAL" => r.try_get_unchecked::<f64, _>(i).map(|f| json!(f)).unwrap_or(Value::Null),
        "BLOB" => {
            let n = r.try_get_unchecked::<Vec<u8>, _>(i).map(|b| b.len()).unwrap_or(0);
            Value::String(format!("[blob {n} bytes]"))
        }
        _ => r.try_get_unchecked::<String, _>(i).map(Value::String).unwrap_or(Value::Null),
    }
}

fn map(e: sqlx::Error) -> AppError {
    // The first line of a database error is enough for the UI and never contains credentials.
    let msg = e.to_string();
    let text = msg.lines().next().unwrap_or("query failed").to_string();
    if text.contains("canceling statement due to statement timeout") || text.contains("maximum statement execution time") {
        return AppError::Timeout(crate::database::session::STATEMENT_TIMEOUT_MS);
    }
    if text.contains("canceling statement due to user request") || text.contains("Query execution was interrupted") {
        return AppError::Cancelled;
    }
    AppError::Introspection(text)
}

/// EXPLAIN without ANALYZE: the statement itself is never executed.
pub async fn explain(session: &Session, sql: &str) -> AppResult<QueryPlan> {
    check_read_only(sql, session.config.engine)?;
    let body = sql.trim().trim_end_matches(';').trim();
    let parsed = {
        use sqlparser::dialect::GenericDialect;
        sqlparser::parser::Parser::parse_sql(&GenericDialect {}, body)
    };
    if let Ok(stmts) = &parsed {
        if !matches!(stmts.first(), Some(sqlparser::ast::Statement::Query(_))) {
            return Err(AppError::Validation("only SELECT queries can be explained".into()));
        }
    }
    match &session.pool {
        Pool::Postgres(p) => {
            let rows = sqlx::raw_sql(AssertSqlSafe(format!("EXPLAIN (FORMAT JSON) {body}"))).fetch_all(p).await.map_err(map)?;
            let text: String = rows.first().and_then(|r| r.try_get_unchecked::<String, _>(0).ok()).ok_or_else(|| AppError::Introspection("EXPLAIN returned nothing".into()))?;
            plan::parse_postgres(&text)
        }
        Pool::Mysql(p) => {
            let rows = sqlx::raw_sql(AssertSqlSafe(format!("EXPLAIN FORMAT=JSON {body}"))).fetch_all(p).await.map_err(map)?;
            let text = rows.first().and_then(|r| mysql_text(r, 0)).ok_or_else(|| AppError::Introspection("EXPLAIN returned nothing".into()))?;
            plan::parse_mysql(&text)
        }
        Pool::Sqlite(p) => {
            let rows = sqlx::raw_sql(AssertSqlSafe(format!("EXPLAIN QUERY PLAN {body}"))).fetch_all(p).await.map_err(map)?;
            let triples: Vec<(i64, i64, String)> = rows
                .iter()
                .map(|r| (r.try_get_unchecked::<i64, _>(0).unwrap_or(0), r.try_get_unchecked::<i64, _>(1).unwrap_or(0), r.try_get_unchecked::<String, _>(3).unwrap_or_default()))
                .collect();
            plan::parse_sqlite(&triples)
        }
    }
}

/// Engine tag helper for the UI.
pub fn engine_supports_cancel(e: Engine) -> bool {
    matches!(e, Engine::Postgres | Engine::Mysql)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_text_values_by_type() {
        assert_eq!(to_json("INT4", Some("42".into())), json!(42));
        assert_eq!(to_json("BIGINT UNSIGNED", Some("18446744073709551615".into())), json!(18446744073709551615u64));
        assert_eq!(to_json("FLOAT8", Some("1.5".into())), json!(1.5));
        assert_eq!(to_json("BOOL", Some("t".into())), json!(true));
        assert_eq!(to_json("JSONB", Some("{\"a\":1}".into())), json!({"a": 1}));
        assert_eq!(to_json("TEXT", Some("hi".into())), json!("hi"));
        assert_eq!(to_json("INT4", None), Value::Null);
    }

    #[test]
    fn numeric_keeps_precision_as_string() {
        assert_eq!(to_json("NUMERIC", Some("12345678901234567890.123456789".into())), json!("12345678901234567890.123456789"));
        assert_eq!(to_json("DECIMAL", Some("0.10".into())), json!("0.10"));
    }

    #[test]
    fn bad_numbers_fall_back_to_strings() {
        assert_eq!(to_json("INT4", Some("abc".into())), json!("abc"));
        assert_eq!(to_json("FLOAT8", Some("NaN".into())), json!("NaN"));
        assert_eq!(to_json("JSON", Some("{broken".into())), json!("{broken"));
    }

    #[test]
    fn limit_is_clamped() {
        assert_eq!(clamp_limit(None), DEFAULT_LIMIT);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(10_000_000)), MAX_LIMIT);
    }
}
