//! A live, read-only connection to a user database.
//!
//! Read-only is enforced at the connection level wherever the engine supports it; the
//! SQL guard is only an additional layer. The best protection remains a database account
//! that only has read privileges.

use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{MySqlPool, PgPool, SqlitePool};

use super::config::{ConnectionConfig, SslMode};
use super::{mysql, postgres, sqlite};
use crate::error::{AppError, AppResult};
use crate::metadata::{DatabaseSchema, Engine};

pub const STATEMENT_TIMEOUT_MS: u64 = 30_000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub enum Pool {
    Postgres(PgPool),
    Mysql(MySqlPool),
    Sqlite(SqlitePool),
}

pub struct Session {
    pub config: ConnectionConfig,
    pub pool: Pool,
    pub server_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProbe {
    pub engine: Engine,
    pub server_version: String,
    pub latency_ms: u64,
    pub read_only: bool,
}

impl Session {
    pub async fn connect(config: &ConnectionConfig, password: Option<&str>) -> AppResult<Session> {
        config.validate()?;
        let pool = match config.engine {
            Engine::Postgres => Pool::Postgres(pg_pool(config, password).await?),
            Engine::Mysql => Pool::Mysql(mysql_pool(config, password).await?),
            Engine::Sqlite => Pool::Sqlite(sqlite_pool(config).await?),
            Engine::Generic => return Err(AppError::Validation("not a connectable engine".into())),
        };
        let server_version = match &pool {
            Pool::Postgres(p) => sqlx::query_scalar::<_, String>("SHOW server_version").fetch_one(p).await,
            Pool::Mysql(p) => sqlx::query_scalar::<_, String>("SELECT CAST(VERSION() AS CHAR)").fetch_one(p).await,
            Pool::Sqlite(p) => sqlx::query_scalar::<_, String>("SELECT sqlite_version()").fetch_one(p).await,
        }
        .map_err(|e| AppError::Connection(clean(e)))?;
        Ok(Session { config: config.clone(), pool, server_version })
    }

    /// Connects, measures a round trip and closes again. Nothing is kept.
    pub async fn probe(config: &ConnectionConfig, password: Option<&str>) -> AppResult<ConnectionProbe> {
        let session = Session::connect(config, password).await?;
        let latency_ms = session.ping().await?;
        let probe = ConnectionProbe {
            engine: config.engine,
            server_version: session.server_version.clone(),
            latency_ms,
            read_only: true,
        };
        session.close().await;
        Ok(probe)
    }

    pub async fn ping(&self) -> AppResult<u64> {
        let start = Instant::now();
        match &self.pool {
            Pool::Postgres(p) => sqlx::query("SELECT 1").execute(p).await.map(|_| ()),
            Pool::Mysql(p) => sqlx::query("SELECT 1").execute(p).await.map(|_| ()),
            Pool::Sqlite(p) => sqlx::query("SELECT 1").execute(p).await.map(|_| ()),
        }
        .map_err(|e| AppError::Connection(clean(e)))?;
        Ok(start.elapsed().as_millis() as u64)
    }

    pub fn database_name(&self) -> String {
        match self.config.engine {
            Engine::Sqlite => self
                .config
                .file_path
                .as_deref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            _ => self.config.database.clone().unwrap_or_default(),
        }
    }

    pub async fn introspect(&self, include_system: bool) -> AppResult<DatabaseSchema> {
        match &self.pool {
            Pool::Postgres(p) => postgres::introspect(p, include_system).await,
            Pool::Mysql(p) => mysql::introspect(p).await,
            Pool::Sqlite(p) => sqlite::introspect(p, &self.database_name()).await,
        }
    }

    pub async fn close(&self) {
        match &self.pool {
            Pool::Postgres(p) => p.close().await,
            Pool::Mysql(p) => p.close().await,
            Pool::Sqlite(p) => p.close().await,
        }
    }
}

async fn pg_pool(c: &ConnectionConfig, password: Option<&str>) -> AppResult<PgPool> {
    let mut o = PgConnectOptions::new()
        .host(c.host.as_deref().unwrap_or_default())
        .port(c.port.unwrap_or(5432))
        .database(c.database.as_deref().unwrap_or_default())
        .username(c.username.as_deref().unwrap_or_default())
        .ssl_mode(match c.ssl_mode {
            SslMode::Disable => PgSslMode::Disable,
            SslMode::Prefer => PgSslMode::Prefer,
            SslMode::Require => PgSslMode::Require,
            SslMode::VerifyCa => PgSslMode::VerifyCa,
            SslMode::VerifyFull => PgSslMode::VerifyFull,
        })
        .application_name("DB-BUTURE")
        .options([
            ("default_transaction_read_only", "on".to_string()),
            ("statement_timeout", STATEMENT_TIMEOUT_MS.to_string()),
            ("idle_in_transaction_session_timeout", "60000".to_string()),
        ]);
    if let Some(pw) = password {
        o = o.password(pw);
    }
    PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(CONNECT_TIMEOUT)
        .connect_with(o)
        .await
        .map_err(|e| AppError::Connection(clean(e)))
}

async fn mysql_pool(c: &ConnectionConfig, password: Option<&str>) -> AppResult<MySqlPool> {
    let mut o = MySqlConnectOptions::new()
        .host(c.host.as_deref().unwrap_or_default())
        .port(c.port.unwrap_or(3306))
        .database(c.database.as_deref().unwrap_or_default())
        .username(c.username.as_deref().unwrap_or_default())
        .ssl_mode(match c.ssl_mode {
            SslMode::Disable => MySqlSslMode::Disabled,
            SslMode::Prefer => MySqlSslMode::Preferred,
            SslMode::Require => MySqlSslMode::Required,
            SslMode::VerifyCa => MySqlSslMode::VerifyCa,
            SslMode::VerifyFull => MySqlSslMode::VerifyIdentity,
        });
    if let Some(pw) = password {
        o = o.password(pw);
    }
    MySqlPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(CONNECT_TIMEOUT)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET SESSION TRANSACTION READ ONLY").execute(&mut *conn).await?;
                sqlx::query("SET SESSION max_execution_time = 30000").execute(&mut *conn).await?;
                Ok(())
            })
        })
        .connect_with(o)
        .await
        .map_err(|e| AppError::Connection(clean(e)))
}

async fn sqlite_pool(c: &ConnectionConfig) -> AppResult<SqlitePool> {
    let path = c.file_path.as_deref().unwrap_or_default();
    if !std::path::Path::new(path).is_file() {
        return Err(AppError::Connection(format!("SQLite file not found: {path}")));
    }
    let o = SqliteConnectOptions::new().filename(path).read_only(true).create_if_missing(false);
    SqlitePoolOptions::new()
        .max_connections(2)
        .acquire_timeout(CONNECT_TIMEOUT)
        .connect_with(o)
        .await
        .map_err(|e| AppError::Connection(clean(e)))
}

/// Error text for the UI. sqlx connect errors never include the password, but we keep
/// only the first line to avoid leaking driver internals.
fn clean(e: sqlx::Error) -> String {
    e.to_string().lines().next().unwrap_or("unknown error").to_string()
}
