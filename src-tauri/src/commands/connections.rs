use serde::Deserialize;
use tauri::State;

use crate::database::{ConnectionConfig, ConnectionProbe, Session};
use crate::error::{AppError, AppResult};
use crate::security::secret_key_for;
use crate::state::{Active, ActiveInfo, AppState};
use crate::storage::connections::SavedConnection;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectInput {
    /// Use a saved connection (password comes from the keyring) ...
    pub saved_id: Option<String>,
    /// ... or an ad-hoc config with the password typed by the user.
    pub config: Option<ConnectionConfig>,
    pub password: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveInput {
    pub id: Option<String>,
    pub name: String,
    pub config: ConnectionConfig,
    /// Written to the OS keyring; absent keeps the existing secret.
    pub password: Option<String>,
}

/// Resolves config + password without ever returning the password to the caller.
async fn resolve(
    state: &AppState,
    saved_id: Option<&str>,
    config: Option<ConnectionConfig>,
    password: Option<String>,
) -> AppResult<(ConnectionConfig, Option<String>, Option<SavedConnection>)> {
    if let Some(id) = saved_id {
        let saved = state
            .storage
            .get_connection(id)
            .await?
            .ok_or_else(|| AppError::Validation("saved connection not found".into()))?;
        let pw = match (password, &saved.secret_key) {
            (Some(p), _) => Some(p),
            (None, Some(key)) => state.credentials.get(key)?,
            (None, None) => None,
        };
        return Ok((saved.config.clone(), pw, Some(saved)));
    }
    let config =
        config.ok_or_else(|| AppError::Validation("connection details are required".into()))?;
    Ok((config, password, None))
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    saved_id: Option<String>,
    config: Option<ConnectionConfig>,
    password: Option<String>,
) -> AppResult<ConnectionProbe> {
    let (config, pw, _) = resolve(&state, saved_id.as_deref(), config, password).await?;
    Session::probe(&config, pw.as_deref()).await
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    input: SaveInput,
) -> AppResult<SavedConnection> {
    input.config.validate()?;
    if input.name.trim().is_empty() {
        return Err(AppError::Validation("connection name is required".into()));
    }
    let existing = match &input.id {
        Some(id) => state.storage.get_connection(id).await?,
        None => None,
    };
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now().to_rfc3339();

    let secret_key = match (&input.password, input.config.engine) {
        (Some(pw), e) if e != crate::metadata::Engine::Sqlite && !pw.is_empty() => {
            let key = secret_key_for(&id);
            state.credentials.save(&key, pw)?;
            Some(key)
        }
        _ => existing.as_ref().and_then(|c| c.secret_key.clone()),
    };

    let saved = SavedConnection {
        id,
        name: input.name.trim().to_string(),
        config: input.config,
        secret_key,
        created_at: existing
            .map(|c| c.created_at)
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    state.storage.save_connection(&saved).await?;
    Ok(saved)
}

#[tauri::command]
pub async fn list_connections(state: State<'_, AppState>) -> AppResult<Vec<SavedConnection>> {
    state.storage.list_connections().await
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> AppResult<()> {
    if let Some(c) = state.storage.get_connection(&id).await? {
        if let Some(key) = c.secret_key {
            state.credentials.delete(&key)?;
        }
    }
    state.storage.delete_connection(&id).await
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, input: ConnectInput) -> AppResult<ActiveInfo> {
    let (config, pw, saved) = resolve(
        &state,
        input.saved_id.as_deref(),
        input.config,
        input.password,
    )
    .await?;
    let session = Session::connect(&config, pw.as_deref()).await?;
    let name = saved
        .as_ref()
        .map(|s| s.name.clone())
        .or(input.name)
        .unwrap_or_else(|| session.database_name());

    let layout_key = session.layout_key();
    state
        .replace_active(Active {
            session: Some(std::sync::Arc::new(session)),
            name,
            connection_id: saved.map(|s| s.id),
            cache: None,
            layout_key,
        })
        .await;
    state.info().await.ok_or(AppError::NotConnected)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> AppResult<()> {
    if let Some(old) = state.active.lock().await.take() {
        if let Some(s) = old.session {
            s.close().await;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn connection_status(state: State<'_, AppState>) -> AppResult<Option<ActiveInfo>> {
    Ok(state.info().await)
}
