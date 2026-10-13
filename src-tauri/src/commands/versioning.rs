use serde::{Deserialize, Serialize};
use tauri::State;

use crate::analysis::centrality::{self, TableCentrality};
use crate::analysis::diff::{diff_schemas, SchemaDiff};
use crate::analysis::lineage::{self, TableLineage, ViewColumnLineage};
use crate::analysis::migration::{self, MigrationReport};
use crate::analysis::snapshot::sha256_hex;
use crate::database::Session;
use crate::error::{AppError, AppResult};
use crate::export;
use crate::metadata::StableId;
use crate::state::AppState;
use crate::storage::snapshots::SnapshotRow;

fn parse_id(s: String) -> AppResult<StableId> {
    serde_json::from_value(serde_json::Value::String(s))
        .map_err(|e| AppError::Validation(e.to_string()))
}

#[tauri::command]
pub async fn create_snapshot(
    state: State<'_, AppState>,
    name: Option<String>,
) -> AppResult<SnapshotRow> {
    let cached = state.current().await?;
    let connection_id = state
        .active
        .lock()
        .await
        .as_ref()
        .and_then(|a| a.connection_id.clone());
    let id = uuid::Uuid::new_v4().to_string();
    let hash = sha256_hex(&cached.schema);
    let name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            format!(
                "{} · {}",
                cached.schema.database,
                chrono::Utc::now().format("%Y-%m-%d %H:%M")
            )
        });
    state
        .storage
        .save_snapshot(&id, &name, connection_id.as_deref(), &hash, &cached.schema)
        .await?;
    state
        .storage
        .get_snapshot(&id)
        .await?
        .map(|(row, _)| row)
        .ok_or_else(|| AppError::Storage("snapshot was not saved".into()))
}

#[tauri::command]
pub async fn list_snapshots(state: State<'_, AppState>) -> AppResult<Vec<SnapshotRow>> {
    state.storage.list_snapshots().await
}

#[tauri::command]
pub async fn rename_snapshot(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    state.storage.rename_snapshot(&id, name.trim()).await
}

#[tauri::command]
pub async fn delete_snapshot(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.storage.delete_snapshot(&id).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotContent {
    pub snapshot: SnapshotRow,
    pub schema: crate::metadata::DatabaseSchema,
}

/// Opens a stored snapshot for viewing. It is clearly historical data, never the current state.
#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>, id: String) -> AppResult<SnapshotContent> {
    let (snapshot, schema) = state
        .storage
        .get_snapshot(&id)
        .await?
        .ok_or_else(|| AppError::Validation("snapshot not found".into()))?;
    Ok(SnapshotContent { snapshot, schema })
}

#[tauri::command]
pub async fn export_snapshot(state: State<'_, AppState>, id: String) -> AppResult<String> {
    let (_, schema) = state
        .storage
        .get_snapshot(&id)
        .await?
        .ok_or_else(|| AppError::Validation("snapshot not found".into()))?;
    Ok(export::schema_to_json(&schema))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReport {
    pub before_label: String,
    pub after_label: String,
    pub before_sha: String,
    pub after_sha: String,
    pub identical: bool,
    pub diff: SchemaDiff,
}

/// `before`/`after` are snapshot ids, or the literal `"current"` for the loaded schema.
#[tauri::command]
pub async fn diff_versions(
    state: State<'_, AppState>,
    before: String,
    after: String,
) -> AppResult<DiffReport> {
    async fn load(
        state: &AppState,
        which: &str,
    ) -> AppResult<(String, crate::metadata::DatabaseSchema)> {
        if which == "current" {
            let c = state.current().await?;
            Ok(("current (loaded metadata)".to_string(), c.schema.clone()))
        } else {
            let (row, schema) = state
                .storage
                .get_snapshot(which)
                .await?
                .ok_or_else(|| AppError::Validation("snapshot not found".into()))?;
            Ok((format!("snapshot “{}”", row.name), schema))
        }
    }
    let (bl, b) = load(&state, &before).await?;
    let (al, a) = load(&state, &after).await?;
    let (bs, as_) = (sha256_hex(&b), sha256_hex(&a));
    Ok(DiffReport {
        before_label: bl,
        after_label: al,
        identical: bs == as_,
        before_sha: bs,
        after_sha: as_,
        diff: diff_schemas(&b, &a),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvInput {
    pub saved_id: String,
    pub password: Option<String>,
}

/// Compares the *structure* of the connected database (before) with another saved connection
/// (after). No table data is read. The second connection is opened read-only and closed again.
#[tauri::command]
pub async fn compare_environment(
    state: State<'_, AppState>,
    input: EnvInput,
) -> AppResult<DiffReport> {
    let current = state.current().await?;
    let saved = state
        .storage
        .get_connection(&input.saved_id)
        .await?
        .ok_or_else(|| AppError::Validation("saved connection not found".into()))?;
    let password = match (input.password, &saved.secret_key) {
        (Some(p), _) => Some(p),
        (None, Some(k)) => state.credentials.get(k)?,
        (None, None) => None,
    };
    let other = Session::connect(&saved.config, password.as_deref()).await?;
    let schema = other.introspect(false).await;
    other.close().await;
    let schema = schema?;
    let before_name = state
        .info()
        .await
        .map(|i| i.connection_name)
        .unwrap_or_else(|| "current".into());
    let (bs, as_) = (sha256_hex(&current.schema), sha256_hex(&schema));
    Ok(DiffReport {
        before_label: before_name,
        after_label: saved.name,
        identical: bs == as_,
        before_sha: bs,
        after_sha: as_,
        diff: diff_schemas(&current.schema, &schema),
    })
}

#[tauri::command]
pub async fn simulate_migration(
    state: State<'_, AppState>,
    ddl: String,
) -> AppResult<MigrationReport> {
    let cached = state.current().await?;
    Ok(migration::simulate(&cached.schema, &ddl))
}

#[tauri::command]
pub async fn table_centrality(state: State<'_, AppState>) -> AppResult<Vec<TableCentrality>> {
    let cached = state.current().await?;
    Ok(centrality::centrality(&cached.schema))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineageReport {
    pub tables: TableLineage,
    pub columns: ViewColumnLineage,
}

#[tauri::command]
pub async fn view_lineage(state: State<'_, AppState>, view_id: String) -> AppResult<LineageReport> {
    let cached = state.current().await?;
    let id = parse_id(view_id)?;
    let view = cached
        .schema
        .views
        .iter()
        .find(|v| v.stable_id == id)
        .ok_or_else(|| AppError::Validation("view not found".into()))?;
    Ok(LineageReport {
        tables: lineage::table_lineage(&cached.schema, view),
        columns: lineage::column_lineage(&cached.schema, view),
    })
}

#[tauri::command]
pub async fn export_schema(state: State<'_, AppState>, format: String) -> AppResult<String> {
    let cached = state.current().await?;
    match format.as_str() {
        "json" => Ok(export::schema_to_json(&cached.schema)),
        "markdown" => Ok(export::schema_to_markdown(&cached.schema)),
        other => Err(AppError::Validation(format!(
            "unknown export format `{other}`"
        ))),
    }
}

/// Writes an export to a path the user picked in a save dialog. Only known extensions are
/// accepted and `base64` content (PNG) is decoded before writing.
#[tauri::command]
pub async fn save_export(path: String, content: String, base64: Option<bool>) -> AppResult<()> {
    use std::io::Write;
    let p = std::path::Path::new(&path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    if !["json", "md", "svg", "png", "sql"].contains(&ext.as_str()) {
        return Err(AppError::Validation(format!(
            "exports may only be saved as json, md, svg, png or sql (got .{ext})"
        )));
    }
    let bytes = if base64.unwrap_or(false) {
        decode_base64(&content)
            .ok_or_else(|| AppError::Validation("invalid base64 content".into()))?
    } else {
        content.into_bytes()
    };
    let mut f = std::fs::File::create(p).map_err(|e| AppError::Storage(e.to_string()))?;
    f.write_all(&bytes)
        .map_err(|e| AppError::Storage(e.to_string()))
}

fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in s.bytes().filter(|b| !b.is_ascii_whitespace()) {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decoding() {
        assert_eq!(decode_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_base64("aGVs\nbG8=").unwrap(), b"hello");
        assert!(decode_base64("a$b").is_none());
    }

    #[tokio::test]
    async fn save_export_rejects_unknown_extensions() {
        assert!(save_export("/tmp/x.sh".into(), "echo".into(), None)
            .await
            .is_err());
        assert!(save_export("/tmp/noext".into(), "x".into(), None)
            .await
            .is_err());
    }
}
