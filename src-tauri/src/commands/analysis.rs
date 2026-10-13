use serde::Serialize;
use tauri::State;

use crate::analysis::audit::{self, Finding, HealthReport, RuleInfo};
use crate::analysis::impact::{self, ImpactReport};
use crate::analysis::indexes::{self, FkIndexGap, IndexRedundancy, IndexSuggestion};
use crate::error::{AppError, AppResult};
use crate::metadata::StableId;
use crate::state::AppState;

fn parse_id(s: String) -> AppResult<StableId> {
    serde_json::from_value(serde_json::Value::String(s))
        .map_err(|e| AppError::Validation(e.to_string()))
}

#[tauri::command]
pub async fn analyze_impact(
    state: State<'_, AppState>,
    object_id: String,
) -> AppResult<ImpactReport> {
    let cached = state.current().await?;
    let id = parse_id(object_id)?;
    impact::analyze_impact(&cached.schema, &cached.graph, &id)
        .ok_or_else(|| AppError::Validation("object not found in the loaded schema".into()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub findings: Vec<Finding>,
    pub health: HealthReport,
    pub rules: Vec<RuleInfo>,
}

#[tauri::command]
pub async fn run_audit(state: State<'_, AppState>) -> AppResult<AuditReport> {
    let cached = state.current().await?;
    let findings = audit::run_audit(&cached.schema, &cached.graph);
    let health = audit::health(&cached.schema, &findings);
    Ok(AuditReport {
        findings,
        health,
        rules: audit::rule_catalog(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexReport {
    pub missing_fk_indexes: Vec<FkIndexGap>,
    pub redundant: Vec<IndexRedundancy>,
    pub suggestions: Vec<IndexSuggestion>,
}

#[tauri::command]
pub async fn index_analysis(state: State<'_, AppState>) -> AppResult<IndexReport> {
    let cached = state.current().await?;
    Ok(IndexReport {
        missing_fk_indexes: indexes::fk_without_index(&cached.schema),
        redundant: indexes::redundant_indexes(&cached.schema),
        suggestions: indexes::suggestions(&cached.schema),
    })
}
