use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::graph::GraphView;
use crate::metadata::{DatabaseSchema, StableId};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaPayload {
    pub schema: DatabaseSchema,
    /// RFC 3339 time of the introspection. The UI must show how old this is.
    pub fetched_at: String,
    pub fingerprint: String,
}

#[tauri::command]
pub async fn load_schema(
    state: State<'_, AppState>,
    refresh: Option<bool>,
    include_system: Option<bool>,
) -> AppResult<SchemaPayload> {
    let cached = state
        .schema(refresh.unwrap_or(false), include_system.unwrap_or(false))
        .await?;
    Ok(SchemaPayload {
        schema: cached.schema.clone(),
        fetched_at: cached.fetched_at.clone(),
        fingerprint: cached.fingerprint.clone(),
    })
}

#[tauri::command]
pub async fn dependency_graph(state: State<'_, AppState>) -> AppResult<GraphView> {
    Ok(state.current().await?.graph.view())
}

#[tauri::command]
pub async fn neighborhood(
    state: State<'_, AppState>,
    id: String,
    hops: Option<usize>,
) -> AppResult<Vec<StableId>> {
    let cached = state.current().await?;
    let id: StableId = serde_json::from_value(serde_json::Value::String(id))
        .map_err(|e| AppError::Validation(e.to_string()))?;
    Ok(cached.graph.neighborhood(&id, hops))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathResult {
    pub shortest: Option<Vec<StableId>>,
    pub alternatives: Vec<Vec<StableId>>,
}

#[tauri::command]
pub async fn find_path(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> AppResult<PathResult> {
    let cached = state.current().await?;
    let parse = |s: String| {
        serde_json::from_value::<StableId>(serde_json::Value::String(s))
            .map_err(|e| AppError::Validation(e.to_string()))
    };
    let (a, b) = (parse(from)?, parse(to)?);
    Ok(PathResult {
        shortest: cached.graph.shortest_path(&a, &b),
        alternatives: cached.graph.alternative_paths(&a, &b, 8, 5),
    })
}
