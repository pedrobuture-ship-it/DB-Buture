use serde::Serialize;
use sqlx::Row;

use super::Storage;
use crate::error::{AppError, AppResult};
use crate::metadata::DatabaseSchema;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRow {
    pub id: String,
    pub name: String,
    pub connection_id: Option<String>,
    pub sha256: String,
    pub created_at: String,
    pub tables: usize,
    pub views: usize,
    pub engine: String,
    pub database: String,
}

impl Storage {
    pub async fn save_snapshot(
        &self,
        id: &str,
        name: &str,
        connection_id: Option<&str>,
        sha256: &str,
        schema: &DatabaseSchema,
    ) -> AppResult<()> {
        let payload =
            serde_json::to_string(schema).map_err(|e| AppError::Storage(e.to_string()))?;
        sqlx::query("INSERT INTO snapshots (id, connection_id, name, sha256, payload, created_at) VALUES (?,?,?,?,?,?)")
            .bind(id)
            .bind(connection_id)
            .bind(name)
            .bind(sha256)
            .bind(payload)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_snapshots(&self) -> AppResult<Vec<SnapshotRow>> {
        let rows = sqlx::query("SELECT id, name, connection_id, sha256, created_at, payload FROM snapshots ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| {
                let payload: String = r.try_get("payload")?;
                // Only the header of the payload is needed for listing.
                let v: serde_json::Value =
                    serde_json::from_str(&payload).map_err(|e| AppError::Storage(e.to_string()))?;
                let count = |k: &str| v.get(k).and_then(|x| x.as_array()).map_or(0, |a| a.len());
                Ok(SnapshotRow {
                    id: r.try_get("id")?,
                    name: r.try_get("name")?,
                    connection_id: r.try_get("connection_id")?,
                    sha256: r.try_get("sha256")?,
                    created_at: r.try_get("created_at")?,
                    tables: count("tables"),
                    views: count("views"),
                    engine: v
                        .get("engine")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    database: v
                        .get("database")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
            })
            .collect()
    }

    pub async fn get_snapshot(&self, id: &str) -> AppResult<Option<(SnapshotRow, DatabaseSchema)>> {
        let Some(r) = sqlx::query("SELECT id, name, connection_id, sha256, created_at, payload FROM snapshots WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };
        let payload: String = r.try_get("payload")?;
        let schema: DatabaseSchema = serde_json::from_str(&payload)
            .map_err(|e| AppError::Storage(format!("snapshot is corrupt: {e}")))?;
        let row = SnapshotRow {
            id: r.try_get("id")?,
            name: r.try_get("name")?,
            connection_id: r.try_get("connection_id")?,
            sha256: r.try_get("sha256")?,
            created_at: r.try_get("created_at")?,
            tables: schema.tables.len(),
            views: schema.views.len(),
            engine: schema.engine.slug().to_string(),
            database: schema.database.clone(),
        };
        Ok(Some((row, schema)))
    }

    pub async fn rename_snapshot(&self, id: &str, name: &str) -> AppResult<()> {
        sqlx::query("UPDATE snapshots SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_snapshot(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM snapshots WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::snapshot::sha256_hex;
    use crate::graph::test_support::sample;

    #[tokio::test]
    async fn snapshot_lifecycle() {
        let s = Storage::open_in_memory().await.unwrap();
        let schema = sample();
        let hash = sha256_hex(&schema);
        s.save_snapshot("a", "before", None, &hash, &schema)
            .await
            .unwrap();
        s.save_snapshot("b", "after", Some("conn"), &hash, &schema)
            .await
            .unwrap();

        let list = s.list_snapshots().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].tables, 4);
        assert_eq!(list[0].engine, "postgres");

        let (row, loaded) = s.get_snapshot("a").await.unwrap().unwrap();
        assert_eq!(row.name, "before");
        assert_eq!(loaded, schema);
        assert_eq!(
            sha256_hex(&loaded),
            hash,
            "a stored snapshot hashes identically after a roundtrip"
        );

        s.rename_snapshot("a", "renamed").await.unwrap();
        assert_eq!(
            s.get_snapshot("a").await.unwrap().unwrap().0.name,
            "renamed"
        );
        s.delete_snapshot("a").await.unwrap();
        assert!(s.get_snapshot("a").await.unwrap().is_none());
        assert!(s.get_snapshot("zzz").await.unwrap().is_none());
    }
}
