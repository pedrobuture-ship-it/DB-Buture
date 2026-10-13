pub mod analysis;
pub mod commands;
pub mod database;
pub mod error;
pub mod export;
pub mod graph;
pub mod importer;
pub mod metadata;
pub mod plan;
pub mod query;
pub mod security;
pub mod state;
pub mod storage;

use std::sync::Arc;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use security::KeyringStore;
use state::AppState;
use storage::Storage;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let storage =
                tauri::async_runtime::block_on(Storage::open(&dir.join("db-buture.sqlite")))
                    .map_err(|e| e.to_string())?;
            app.manage(AppState::new(storage, Arc::new(KeyringStore)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::connections::test_connection,
            commands::connections::save_connection,
            commands::connections::list_connections,
            commands::connections::delete_connection,
            commands::connections::connect,
            commands::connections::disconnect,
            commands::connections::connection_status,
            commands::schema::load_schema,
            commands::schema::dependency_graph,
            commands::schema::neighborhood,
            commands::schema::find_path,
            commands::schema::get_layout,
            commands::schema::save_layout,
            commands::schema::clear_layout,
            commands::analysis::analyze_impact,
            commands::analysis::run_audit,
            commands::analysis::index_analysis,
            commands::sql::import_sql,
            commands::sql::open_import,
            commands::sql::run_query,
            commands::sql::cancel_query,
            commands::sql::explain_query,
            commands::sql::capabilities,
            commands::versioning::create_snapshot,
            commands::versioning::list_snapshots,
            commands::versioning::rename_snapshot,
            commands::versioning::delete_snapshot,
            commands::versioning::get_snapshot,
            commands::versioning::export_snapshot,
            commands::versioning::diff_versions,
            commands::versioning::compare_environment,
            commands::versioning::simulate_migration,
            commands::versioning::table_centrality,
            commands::versioning::view_lineage,
            commands::versioning::export_schema,
            commands::versioning::save_export,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DB-BUTURE");
}
