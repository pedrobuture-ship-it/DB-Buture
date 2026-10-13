pub mod connections;
pub mod layouts;
pub mod migrations;

use std::path::Path;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use migrations::MIGRATIONS;

/// Internal SQLite used only by DB-BUTURE itself.
#[derive(Clone)]
pub struct Storage {
    pub(crate) pool: SqlitePool,
}

impl Storage {
    pub async fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Storage(e.to_string()))?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        Self::connect(opts, 4).await
    }

    pub async fn open_in_memory() -> AppResult<Self> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?;
        // A single connection keeps the in-memory database alive and shared.
        Self::connect(opts, 1).await
    }

    async fn connect(opts: SqliteConnectOptions, max: u32) -> AppResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max)
            .connect_with(opts)
            .await?;
        let storage = Self { pool };
        storage.migrate().await?;
        Ok(storage)
    }

    async fn migrate(&self) -> AppResult<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at TEXT NOT NULL)",
        )
        .execute(&self.pool)
        .await?;

        let current: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_migrations")
                .fetch_one(&self.pool)
                .await?;

        for (version, name, sql) in MIGRATIONS.iter().filter(|(v, _, _)| *v > current) {
            let mut tx = self.pool.begin().await?;
            sqlx::raw_sql(*sql).execute(&mut *tx).await?;
            sqlx::query(
                "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?, ?, ?)",
            )
            .bind(version)
            .bind(name)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            tracing::info!(version, name, "applied internal migration");
        }
        Ok(())
    }

    pub async fn schema_version(&self) -> AppResult<i64> {
        Ok(
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_migrations")
                .fetch_one(&self.pool)
                .await?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_apply_and_are_idempotent() {
        let s = Storage::open_in_memory().await.unwrap();
        assert_eq!(s.schema_version().await.unwrap(), MIGRATIONS.len() as i64);
        s.migrate().await.unwrap();
        assert_eq!(s.schema_version().await.unwrap(), MIGRATIONS.len() as i64);
    }

    #[tokio::test]
    async fn no_password_column_exists() {
        let s = Storage::open_in_memory().await.unwrap();
        let cols: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('saved_connections')")
                .fetch_all(&s.pool)
                .await
                .unwrap();
        assert!(!cols.iter().any(|c| c.contains("password")));
        assert!(cols.contains(&"secret_key".to_string()));
    }
}
