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
    let result = cached
        .centrality
        .get_or_try_init(|| async {
            let c = cached.clone();
            tokio::task::spawn_blocking(move || {
                std::sync::Arc::new(centrality::centrality(&c.schema))
            })
            .await
            .map_err(|e| AppError::Internal(e.to_string()))
        })
        .await?;
    Ok((**result).clone())
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

const EXPORT_EXTENSIONS: [&str; 5] = ["json", "md", "svg", "png", "sql"];

fn export_extension(name: &str) -> AppResult<String> {
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    if EXPORT_EXTENSIONS.contains(&ext.as_str()) {
        Ok(ext)
    } else {
        Err(AppError::Validation(format!(
            "exports may only be saved as json, md, svg, png or sql (got .{ext})"
        )))
    }
}

/// Writes export bytes. The destination always comes from a native save dialog opened by Rust,
/// never from the webview, so the interface cannot write to arbitrary paths.
fn write_export(path: &std::path::Path, content: String, base64: bool) -> AppResult<()> {
    use std::io::Write;
    export_extension(&path.to_string_lossy())?;
    let bytes = if base64 {
        decode_base64(&content)
            .ok_or_else(|| AppError::Validation("invalid base64 content".into()))?
    } else {
        content.into_bytes()
    };
    let mut f = std::fs::File::create(path).map_err(|e| AppError::Storage(e.to_string()))?;
    f.write_all(&bytes)
        .map_err(|e| AppError::Storage(e.to_string()))
}

/// Opens a native "save as" dialog and writes `content` where the user chooses.
/// Returns the chosen path, or `None` when the dialog was cancelled.
#[tauri::command]
pub async fn save_export_as(
    app: tauri::AppHandle,
    suggested_name: String,
    content: String,
    base64: Option<bool>,
) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let ext = export_extension(&suggested_name)?;
    let name = suggested_name.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_file_name(name)
            .add_filter(ext.to_uppercase(), &[ext.as_str()])
            .blocking_save_file()
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;
    let Some(path) = chosen else { return Ok(None) };
    let path = path
        .into_path()
        .map_err(|e| AppError::Validation(e.to_string()))?;
    write_export(&path, content, base64.unwrap_or(false))?;
    Ok(Some(path.display().to_string()))
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

    #[test]
    fn exports_are_limited_to_known_extensions() {
        assert_eq!(export_extension("schema.JSON").unwrap(), "json");
        assert!(export_extension("run.sh").is_err());
        assert!(export_extension("noext").is_err());
        let dir = std::env::temp_dir();
        assert!(write_export(&dir.join("x.sh"), "echo".into(), false).is_err());
    }

    #[test]
    fn write_export_writes_text_and_decoded_base64() {
        let dir = std::env::temp_dir().join(format!("db-buture-exp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_export(&dir.join("a.md"), "# hi".into(), false).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.md")).unwrap(), "# hi");
        write_export(&dir.join("b.png"), "aGVsbG8=".into(), true).unwrap();
        assert_eq!(std::fs::read(dir.join("b.png")).unwrap(), b"hello");
        assert!(write_export(&dir.join("c.png"), "%%%".into(), true).is_err());
    }
}
