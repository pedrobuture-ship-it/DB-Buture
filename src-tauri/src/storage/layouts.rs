use serde_json::Value;
use sqlx::Row;

use super::Storage;
use crate::error::{AppError, AppResult};

impl Storage {
    pub async fn get_layout(&self, key: &str, view_kind: &str) -> AppResult<Option<Value>> {
        let row =
            sqlx::query("SELECT layout FROM graph_layouts WHERE fingerprint = ? AND view_kind = ?")
                .bind(key)
                .bind(view_kind)
                .fetch_optional(&self.pool)
                .await?;
        match row {
            Some(r) => {
                let text: String = r.try_get("layout")?;
                serde_json::from_str(&text)
                    .map(Some)
                    .map_err(|e| AppError::Storage(e.to_string()))
            }
            None => Ok(None),
        }
    }

    pub async fn save_layout(&self, key: &str, view_kind: &str, layout: &Value) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO graph_layouts (fingerprint, view_kind, layout, updated_at) VALUES (?,?,?,?)
             ON CONFLICT(fingerprint, view_kind) DO UPDATE SET layout = excluded.layout, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(view_kind)
        .bind(layout.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_layout(&self, key: &str, view_kind: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM graph_layouts WHERE fingerprint = ? AND view_kind = ?")
            .bind(key)
            .bind(view_kind)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn layouts_roundtrip_and_overwrite() {
        let s = Storage::open_in_memory().await.unwrap();
        assert!(s.get_layout("k", "erd").await.unwrap().is_none());
        s.save_layout("k", "erd", &json!({"manual": true}))
            .await
            .unwrap();
        s.save_layout("k", "erd", &json!({"manual": false}))
            .await
            .unwrap();
        assert_eq!(
            s.get_layout("k", "erd").await.unwrap().unwrap()["manual"],
            false
        );
        assert!(s.get_layout("k", "deps").await.unwrap().is_none());
        s.delete_layout("k", "erd").await.unwrap();
        assert!(s.get_layout("k", "erd").await.unwrap().is_none());
    }
}
