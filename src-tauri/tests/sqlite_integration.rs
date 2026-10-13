use db_buture_lib::database::sqlite;
use db_buture_lib::metadata::{DependencyKind, ReferentialAction};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

const FIXTURE: &str = r#"
CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  name VARCHAR(80),
  full_name TEXT GENERATED ALWAYS AS (name || '!') STORED
);
CREATE TABLE orders (
  id INTEGER PRIMARY KEY,
  user_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
  total NUMERIC NOT NULL
);
CREATE TABLE order_items (
  order_id INTEGER NOT NULL,
  line INTEGER NOT NULL,
  sku TEXT,
  PRIMARY KEY (order_id, line),
  FOREIGN KEY (order_id) REFERENCES orders
);
CREATE TABLE item_notes (
  order_id INTEGER, line INTEGER, note TEXT,
  FOREIGN KEY (order_id, line) REFERENCES order_items(order_id, line)
);
CREATE INDEX idx_orders_user ON orders(user_id);
CREATE INDEX idx_users_name_partial ON users(name) WHERE name IS NOT NULL;
CREATE INDEX idx_users_lower ON users(lower(email));
CREATE VIEW user_totals AS SELECT u.id, sum(o.total) AS total FROM users u JOIN orders o ON o.user_id = u.id GROUP BY u.id;
CREATE VIEW top_users AS SELECT * FROM user_totals WHERE total > 100;
CREATE TRIGGER trg_orders_ai AFTER INSERT ON orders BEGIN SELECT 1; END;
"#;

async fn fixture() -> sqlx::SqlitePool {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("db-buture-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fixture.db");
    let w = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&path).create_if_missing(true))
        .await
        .unwrap();
    sqlx::raw_sql(FIXTURE).execute(&w).await.unwrap();
    w.close().await;
    // introspection always uses a read-only connection
    SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&path).read_only(true))
        .await
        .unwrap()
}

#[tokio::test]
async fn introspects_sqlite_file() {
    let pool = fixture().await;
    let s = sqlite::introspect(&pool, "fixture.db").await.unwrap();

    assert_eq!(s.tables.len(), 4);
    let users = s.tables.iter().find(|t| t.name == "users").unwrap();
    assert_eq!(users.primary_key.as_ref().unwrap().columns, ["id"]);
    assert_eq!(users.unique_constraints.len(), 1);
    assert_eq!(users.unique_constraints[0].columns, ["email"]);
    assert_eq!(users.columns.iter().find(|c| c.name == "name").unwrap().data_type, "varchar");
    assert_eq!(users.columns.iter().find(|c| c.name == "full_name").unwrap().generated.as_deref(), Some("stored"));

    let orders = s.tables.iter().find(|t| t.name == "orders").unwrap();
    assert_eq!(orders.foreign_keys[0].on_delete, ReferentialAction::Cascade);

    let items = s.tables.iter().find(|t| t.name == "order_items").unwrap();
    assert_eq!(items.primary_key.as_ref().unwrap().columns, ["order_id", "line"]);
    // implicit target (REFERENCES orders) resolves to orders' primary key
    assert_eq!(items.foreign_keys[0].target_columns, ["id"]);

    let notes = s.tables.iter().find(|t| t.name == "item_notes").unwrap();
    assert_eq!(notes.foreign_keys[0].source_columns, ["order_id", "line"]);
    assert_eq!(notes.foreign_keys[0].target_columns, ["order_id", "line"]);

    let partial = users.indexes.iter().find(|i| i.name == "idx_users_name_partial").unwrap();
    assert_eq!(partial.predicate.as_deref(), Some("name IS NOT NULL"));
    let expr = users.indexes.iter().find(|i| i.name == "idx_users_lower").unwrap();
    assert!(expr.columns.is_empty());
    assert_eq!(expr.engine_specific_metadata["hasExpression"], true);
}

#[tokio::test]
async fn views_triggers_and_dependencies() {
    let pool = fixture().await;
    let s = sqlite::introspect(&pool, "fixture.db").await.unwrap();
    assert_eq!(s.views.len(), 2);
    assert_eq!(s.triggers.len(), 1);
    assert_eq!(s.triggers[0].timing, "AFTER");
    assert_eq!(s.triggers[0].events, ["INSERT"]);
    let dep = |from: &str, to: &str| {
        s.dependencies.iter().any(|d| d.kind == DependencyKind::DependsOn && d.from.as_str().ends_with(from) && d.to.as_str().ends_with(to))
    };
    assert!(dep("view:main.user_totals", "main.users"));
    assert!(dep("view:main.user_totals", "main.orders"));
    assert!(dep("view:main.top_users", "view:main.user_totals"));
}

#[tokio::test]
async fn read_only_connection_rejects_writes() {
    let pool = fixture().await;
    let err = sqlx::query("CREATE TABLE nope (x INT)").execute(&pool).await;
    assert!(err.is_err());
    let err = sqlx::query("DELETE FROM users").execute(&pool).await;
    assert!(err.is_err());
}
