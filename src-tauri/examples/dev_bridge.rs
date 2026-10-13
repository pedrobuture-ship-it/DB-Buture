//! Test-only HTTP bridge that exposes the real Tauri commands to a browser.
//!
//! It lets the UI be exercised end to end (headless Chromium + a small `invoke` shim) against
//! real databases without a webview driver. It uses an in-memory credential store, binds to
//! loopback only and is never part of the shipped application.
//!
//!     cargo run --example dev_bridge            # listens on 127.0.0.1:1430

use std::sync::Arc;

use db_buture_lib::commands::{self, analysis, connections, schema};
use db_buture_lib::error::AppError;
use db_buture_lib::security::MemoryStore;
use db_buture_lib::state::AppState;
use db_buture_lib::storage::Storage;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tauri::Manager;

fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T, AppError> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| AppError::Validation(format!("bad argument `{key}`: {e}")))
}

fn ok<T: serde::Serialize>(r: Result<T, AppError>) -> Result<Value, AppError> {
    r.map(|v| serde_json::to_value(v).unwrap_or(Value::Null))
}

async fn dispatch(
    app: &tauri::App<tauri::test::MockRuntime>,
    cmd: &str,
    a: Value,
) -> Result<Value, AppError> {
    let st = || app.state::<AppState>();
    match cmd {
        "app_info" => Ok(serde_json::to_value(commands::app_info()).unwrap_or(Value::Null)),
        "test_connection" => ok(connections::test_connection(
            st(),
            arg(&a, "savedId")?,
            arg(&a, "config")?,
            arg(&a, "password")?,
        )
        .await),
        "save_connection" => ok(connections::save_connection(st(), arg(&a, "input")?).await),
        "list_connections" => ok(connections::list_connections(st()).await),
        "delete_connection" => ok(connections::delete_connection(st(), arg(&a, "id")?).await),
        "connect" => ok(connections::connect(st(), arg(&a, "input")?).await),
        "disconnect" => ok(connections::disconnect(st()).await),
        "connection_status" => ok(connections::connection_status(st()).await),
        "load_schema" => {
            ok(schema::load_schema(st(), arg(&a, "refresh")?, arg(&a, "includeSystem")?).await)
        }
        "dependency_graph" => ok(schema::dependency_graph(st()).await),
        "neighborhood" => ok(schema::neighborhood(
            st(),
            arg(&a, "id")?,
            arg(&a, "hops")?,
            arg(&a, "foreignKeysOnly")?,
        )
        .await),
        "get_layout" => ok(schema::get_layout(st(), arg(&a, "viewKind")?).await),
        "save_layout" => {
            ok(schema::save_layout(st(), arg(&a, "viewKind")?, arg(&a, "layout")?).await)
        }
        "clear_layout" => ok(schema::clear_layout(st(), arg(&a, "viewKind")?).await),
        "find_path" => ok(schema::find_path(st(), arg(&a, "from")?, arg(&a, "to")?).await),
        "analyze_impact" => ok(analysis::analyze_impact(st(), arg(&a, "objectId")?).await),
        "run_audit" => ok(analysis::run_audit(st()).await),
        "index_analysis" => ok(analysis::index_analysis(st()).await),
        other => Err(AppError::Validation(format!(
            "dev bridge: unknown command `{other}`"
        ))),
    }
}

fn main() {
    let app = tauri::test::mock_app();
    let dir = std::env::temp_dir().join(format!("db-buture-bridge-{}", std::process::id()));
    let storage =
        tauri::async_runtime::block_on(Storage::open(&dir.join("app.sqlite"))).expect("storage");
    app.manage(AppState::new(storage, Arc::new(MemoryStore::default())));

    let addr = std::env::var("DEV_BRIDGE_ADDR").unwrap_or_else(|_| "127.0.0.1:1430".into());
    let server = tiny_http::Server::http(&addr).expect("bind");
    eprintln!("dev bridge listening on http://{addr}");
    let cors = |r: tiny_http::Response<std::io::Cursor<Vec<u8>>>| {
        r.with_header(
            "Access-Control-Allow-Origin: *"
                .parse::<tiny_http::Header>()
                .unwrap(),
        )
        .with_header(
            "Access-Control-Allow-Headers: *"
                .parse::<tiny_http::Header>()
                .unwrap(),
        )
        .with_header(
            "Content-Type: application/json"
                .parse::<tiny_http::Header>()
                .unwrap(),
        )
    };
    for mut req in server.incoming_requests() {
        if req.method() == &tiny_http::Method::Options {
            let _ = req.respond(cors(tiny_http::Response::from_string("")));
            continue;
        }
        let cmd = req.url().trim_start_matches("/invoke/").to_string();
        let mut body = String::new();
        let _ = req.as_reader().read_to_string(&mut body);
        let args: Value = serde_json::from_str(&body).unwrap_or(json!({}));
        let out = tauri::async_runtime::block_on(dispatch(&app, &cmd, args));
        let payload = match out {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "err": e }),
        };
        let _ = req.respond(cors(tiny_http::Response::from_string(payload.to_string())));
    }
}
