pub mod analysis;
pub mod commands;
pub mod database;
pub mod error;
pub mod graph;
pub mod metadata;
pub mod security;
pub mod storage;

use tracing_subscriber::EnvFilter;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_info])
        .run(tauri::generate_context!())
        .expect("error while running DB-BUTURE");
}
