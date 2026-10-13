pub mod analysis;
pub mod commands;
pub mod database;
pub mod error;
pub mod graph;
pub mod metadata;
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
            commands::analysis::analyze_impact,
            commands::analysis::run_audit,
            commands::analysis::index_analysis,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DB-BUTURE");
}
