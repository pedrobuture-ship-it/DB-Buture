use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::Storage;
use crate::database::{ConnectionConfig, SslMode};
use crate::error::{AppError, AppResult};
use crate::metadata::Engine;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub config: ConnectionConfig,
    /// Keyring reference; the password itself is never stored here.
    #[serde(skip_serializing)]
    pub secret_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn engine_str(e: Engine) -> &'static str {
    e.slug()
}

fn parse_engine(s: &str) -> AppResult<Engine> {
    match s {
        "postgres" => Ok(Engine::Postgres),
        "mysql" => Ok(Engine::Mysql),
        "sqlite" => Ok(Engine::Sqlite),
        other => Err(AppError::Storage(format!("unknown engine '{other}' in saved connection"))),
    }
}

impl Storage {
    pub async fn save_connection(&self, conn: &SavedConnection) -> AppResult<()> {
        conn.config.validate()?;
        sqlx::query(
            "INSERT INTO saved_connections
               (id, name, engine, host, port, database_name, username, ssl_mode, file_path,
                secret_key, created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, engine=excluded.engine, host=excluded.host,
               port=excluded.port, database_name=excluded.database_name,
               username=excluded.username, ssl_mode=excluded.ssl_mode,
               file_path=excluded.file_path, secret_key=excluded.secret_key,
               updated_at=excluded.updated_at",
        )
        .bind(&conn.id)
        .bind(&conn.name)
        .bind(engine_str(conn.config.engine))
        .bind(&conn.config.host)
        .bind(conn.config.port.map(i64::from))
        .bind(&conn.config.database)
        .bind(&conn.config.username)
        .bind(conn.config.ssl_mode.as_str())
        .bind(&conn.config.file_path)
        .bind(&conn.secret_key)
        .bind(&conn.created_at)
        .bind(&conn.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_connections(&self) -> AppResult<Vec<SavedConnection>> {
        let rows = sqlx::query("SELECT * FROM saved_connections ORDER BY name COLLATE NOCASE")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_connection).collect()
    }

    pub async fn get_connection(&self, id: &str) -> AppResult<Option<SavedConnection>> {
        let row = sqlx::query("SELECT * FROM saved_connections WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(row_to_connection).transpose()
    }

    pub async fn delete_connection(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM saved_connections WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn row_to_connection(row: &sqlx::sqlite::SqliteRow) -> AppResult<SavedConnection> {
    let engine = parse_engine(&row.try_get::<String, _>("engine")?)?;
    let ssl: String = row.try_get("ssl_mode")?;
    Ok(SavedConnection {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        config: ConnectionConfig {
            engine,
            host: row.try_get("host")?,
            port: row.try_get::<Option<i64>, _>("port")?.and_then(|p| u16::try_from(p).ok()),
            database: row.try_get("database_name")?,
            username: row.try_get("username")?,
            ssl_mode: SslMode::parse(&ssl).unwrap_or_default(),
            file_path: row.try_get("file_path")?,
        },
        secret_key: row.try_get("secret_key")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str, name: &str) -> SavedConnection {
        SavedConnection {
            id: id.into(),
            name: name.into(),
            config: ConnectionConfig {
                engine: Engine::Postgres,
                host: Some("db.local".into()),
                port: Some(5432),
                database: Some("app".into()),
                username: Some("ro".into()),
                ssl_mode: SslMode::Require,
                file_path: None,
            },
            secret_key: Some(format!("db-buture.connection.{id}")),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn save_list_update_delete() {
        let s = Storage::open_in_memory().await.unwrap();
        s.save_connection(&sample("1", "Staging")).await.unwrap();
        s.save_connection(&sample("2", "alpha")).await.unwrap();

        let list = s.list_connections().await.unwrap();
        assert_eq!(list.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["alpha", "Staging"]);
        assert_eq!(list[1].config.ssl_mode, SslMode::Require);

        let mut c = sample("1", "Staging 2");
        c.config.port = Some(6543);
        s.save_connection(&c).await.unwrap();
        assert_eq!(s.get_connection("1").await.unwrap().unwrap().config.port, Some(6543));

        s.delete_connection("1").await.unwrap();
        assert!(s.get_connection("1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn invalid_config_is_not_persisted() {
        let s = Storage::open_in_memory().await.unwrap();
        let mut c = sample("1", "x");
        c.config.host = None;
        assert!(s.save_connection(&c).await.is_err());
        assert!(s.list_connections().await.unwrap().is_empty());
    }
}
