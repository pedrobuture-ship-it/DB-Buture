//! SQL editor backend against real databases (PG / MySQL via env vars, SQLite always).

use std::sync::Arc;

use db_buture_lib::database::{ConnectionConfig, Session, SslMode};
use db_buture_lib::error::AppError;
use db_buture_lib::metadata::Engine;
use db_buture_lib::query::{explain, run_query};
use serde_json::json;
use tokio::sync::Notify;

fn pg_cfg() -> ConnectionConfig {
    ConnectionConfig {
        engine: Engine::Postgres,
        host: Some("127.0.0.1".into()),
        port: Some(54329),
        database: Some("buture".into()),
        username: Some("postgres".into()),
        ssl_mode: SslMode::Disable,
        file_path: None,
    }
}

fn mysql_cfg() -> ConnectionConfig {
    ConnectionConfig {
        engine: Engine::Mysql,
        host: Some("127.0.0.1".into()),
        port: Some(33069),
        database: Some("shop_fixture".into()),
        username: Some("root".into()),
        ssl_mode: SslMode::Disable,
        file_path: None,
    }
}

fn token() -> Arc<Notify> {
    Arc::new(Notify::new())
}

#[tokio::test]
async fn postgres_runs_typed_queries_and_blocks_writes() {
    if std::env::var("DB_BUTURE_TEST_PG_URL").is_err() {
        return;
    }
    let s = Session::connect(&pg_cfg(), Some("buture_test")).await.unwrap();

    let r = run_query(&s, "SELECT 1::int4 AS n, 'x'::text AS t, true AS b, 1.50::numeric AS d, '{\"a\":1}'::jsonb AS j, NULL::int AS nothing, now() AS ts", None, None, token()).await.unwrap();
    assert_eq!(r.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["n", "t", "b", "d", "j", "nothing", "ts"]);
    assert_eq!(r.rows[0][0], json!(1));
    assert_eq!(r.rows[0][1], json!("x"));
    assert_eq!(r.rows[0][2], json!(true));
    assert_eq!(r.rows[0][3], json!("1.50"));
    assert_eq!(r.rows[0][4], json!({"a": 1}));
    assert!(r.rows[0][5].is_null());
    assert!(!r.truncated);

    // result cap
    let r = run_query(&s, "SELECT generate_series(1, 100) AS n", Some(10), None, token()).await.unwrap();
    assert_eq!(r.row_count, 10);
    assert!(r.truncated);

    // empty result still reports columns
    let r = run_query(&s, "SELECT 1 AS a, 'x'::text AS b WHERE false", None, None, token()).await.unwrap();
    assert_eq!(r.row_count, 0);
    assert_eq!(r.columns.len(), 2);

    // guard + read-only session
    assert!(matches!(run_query(&s, "DELETE FROM shop.users", None, None, token()).await, Err(AppError::ReadOnly(_))));
    assert!(matches!(run_query(&s, "SELECT 1; SELECT 2", None, None, token()).await, Err(AppError::ReadOnly(_))));
}

#[tokio::test]
async fn postgres_timeout_and_cancel() {
    if std::env::var("DB_BUTURE_TEST_PG_URL").is_err() {
        return;
    }
    let s = Arc::new(Session::connect(&pg_cfg(), Some("buture_test")).await.unwrap());

    let started = std::time::Instant::now();
    let err = run_query(&s, "SELECT pg_sleep(20)", None, Some(400), token()).await.unwrap_err();
    assert!(matches!(err, AppError::Timeout(_)), "{err:?}");

    let err = run_query(&s, "SELECT count(*) FROM generate_series(1, 2000000000)", None, Some(500), token()).await.unwrap_err();
    assert!(matches!(err, AppError::Timeout(_)), "{err:?}");
    assert!(started.elapsed().as_secs() < 10);

    // explicit cancellation from another task
    let c = token();
    let c2 = c.clone();
    let s2 = s.clone();
    let h = tokio::spawn(async move { run_query(&s2, "SELECT count(*) FROM generate_series(1, 2000000000)", None, None, c2).await });
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    c.notify_one();
    let err = h.await.unwrap().unwrap_err();
    assert!(matches!(err, AppError::Cancelled), "{err:?}");

    // the session is still usable afterwards and the backend really stopped
    let r = run_query(&s, "SELECT count(*) FROM pg_stat_activity WHERE query LIKE '%generate_series(1, 2000000000)%' AND state = 'active' AND pid <> pg_backend_pid()", None, None, token()).await.unwrap();
    assert_eq!(r.rows[0][0], json!(0));
}

#[tokio::test]
async fn postgres_explain_is_estimate_only() {
    if std::env::var("DB_BUTURE_TEST_PG_URL").is_err() {
        return;
    }
    let s = Session::connect(&pg_cfg(), Some("buture_test")).await.unwrap();
    let plan = explain(&s, "SELECT * FROM shop.orders o JOIN shop.users u ON u.id = o.user_id WHERE o.total > 5").await.unwrap();
    assert!(plan.total_cost.unwrap() > 0.0);
    assert!(plan.notes[0].contains("no ANALYZE"));
    fn count(n: &db_buture_lib::plan::PlanNode) -> usize {
        1 + n.children.iter().map(count).sum::<usize>()
    }
    assert!(count(&plan.root) >= 3);
    assert!(explain(&s, "DELETE FROM shop.users").await.is_err());
    assert!(explain(&s, "SHOW server_version").await.is_err());
}

#[tokio::test]
async fn mysql_runs_queries_and_explains() {
    if std::env::var("DB_BUTURE_TEST_MYSQL_URL").is_err() {
        return;
    }
    let s = Session::connect(&mysql_cfg(), Some("buture_test")).await.unwrap();
    let r = run_query(&s, "SELECT 1 AS n, 'x' AS t, CAST(1.50 AS DECIMAL(5,2)) AS d, NULL AS nothing", None, None, token()).await.unwrap();
    assert_eq!(r.rows[0][0], json!(1));
    assert_eq!(r.rows[0][1], json!("x"));
    assert_eq!(r.rows[0][2], json!("1.50"));
    assert!(r.rows[0][3].is_null());

    let r = run_query(&s, "SELECT * FROM users", Some(5), None, token()).await.unwrap();
    assert_eq!(r.columns[0].name, "id");

    assert!(matches!(run_query(&s, "INSERT INTO users (email) VALUES ('x')", None, None, token()).await, Err(AppError::ReadOnly(_))));
    let plan = explain(&s, "SELECT * FROM orders WHERE total > 5").await.unwrap();
    assert!(plan.total_cost.is_some());
}

async fn sqlite_session() -> Session {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("db-buture-q-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("q.db");
    let w = SqlitePoolOptions::new().connect_with(SqliteConnectOptions::new().filename(&path).create_if_missing(true)).await.unwrap();
    sqlx::raw_sql("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score REAL, data BLOB); CREATE INDEX idx_t_name ON t(name); INSERT INTO t VALUES (1,'a',1.5,x'0102'),(2,NULL,2.5,NULL),(3,'c',3.5,NULL);")
        .execute(&w)
        .await
        .unwrap();
    w.close().await;
    let cfg = ConnectionConfig { engine: Engine::Sqlite, host: None, port: None, database: None, username: None, ssl_mode: SslMode::Disable, file_path: Some(path.to_string_lossy().into_owned()) };
    Session::connect(&cfg, None).await.unwrap()
}

#[tokio::test]
async fn sqlite_runs_queries_and_explains() {
    let s = sqlite_session().await;
    let r = run_query(&s, "SELECT id, name, score, data FROM t ORDER BY id", None, None, token()).await.unwrap();
    assert_eq!(r.row_count, 3);
    assert_eq!(r.rows[0], vec![json!(1), json!("a"), json!(1.5), json!("[blob 2 bytes]")]);
    assert!(r.rows[1][1].is_null());

    let r = run_query(&s, "SELECT id FROM t", Some(2), None, token()).await.unwrap();
    assert!(r.truncated);

    assert!(run_query(&s, "DELETE FROM t", None, None, token()).await.is_err());
    assert!(matches!(run_query(&s, "SELECT * FROM missing_table", None, None, token()).await, Err(AppError::Introspection(_))));

    let plan = explain(&s, "SELECT * FROM t WHERE name = 'a'").await.unwrap();
    assert!(plan.root.children[0].index.as_deref() == Some("idx_t_name"), "{plan:?}");
    let scan = explain(&s, "SELECT * FROM t WHERE score > 1").await.unwrap();
    assert!(scan.observations.iter().any(|o| o.message.contains("scans all of t")));
}
