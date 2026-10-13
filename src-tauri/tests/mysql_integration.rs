//! Runs against a real MySQL. Set DB_BUTURE_TEST_MYSQL_URL to enable, e.g.
//! mysql://root:buture_test@127.0.0.1:33069/shop

use db_buture_lib::database::mysql;
use db_buture_lib::metadata::{DependencyKind, ReferentialAction};
use sqlx::mysql::MySqlPoolOptions;

async fn pool() -> Option<sqlx::MySqlPool> {
    let base = std::env::var("DB_BUTURE_TEST_MYSQL_URL").ok()?;
    let admin = MySqlPoolOptions::new().max_connections(1).connect(&base).await.expect("connect");
    sqlx::raw_sql(include_str!("fixtures/mysql.sql")).execute(&admin).await.expect("fixture");
    admin.close().await;
    let url = base.rsplit_once('/').map(|(h, _)| format!("{h}/shop_fixture")).unwrap();
    Some(MySqlPoolOptions::new().max_connections(2).connect(&url).await.expect("connect fixture db"))
}

#[tokio::test]
async fn introspects_mysql() {
    let Some(pool) = pool().await else { return };
    let s = mysql::introspect(&pool).await.unwrap();
    assert_eq!(s.database, "shop_fixture");

    let users = s.tables.iter().find(|t| t.name == "users").unwrap();
    assert_eq!(users.comment.as_deref(), Some("Registered customers"));
    assert_eq!(users.primary_key.as_ref().unwrap().columns, ["id"]);
    assert_eq!(users.columns[0].identity.as_deref(), Some("auto_increment"));
    assert_eq!(users.columns[0].data_type, "bigint");
    assert_eq!(users.columns.iter().find(|c| c.name == "name").unwrap().comment.as_deref(), Some("Display name"));
    assert_eq!(users.unique_constraints[0].columns, ["email"]);

    let orders = s.tables.iter().find(|t| t.name == "orders").unwrap();
    assert_eq!(orders.foreign_keys[0].on_delete, ReferentialAction::Cascade);
    assert_eq!(orders.foreign_keys[0].on_update, ReferentialAction::Restrict);
    assert_eq!(orders.check_constraints.len(), 1);
    assert_eq!(orders.columns.iter().find(|c| c.name == "net").unwrap().generated.as_deref(), Some("stored"));

    let items = s.tables.iter().find(|t| t.name == "order_items").unwrap();
    assert_eq!(items.primary_key.as_ref().unwrap().columns, ["order_id", "line"]);
    assert_eq!(items.unique_constraints[0].columns, ["order_id", "sku"]);

    let notes = s.tables.iter().find(|t| t.name == "item_notes").unwrap();
    assert_eq!(notes.foreign_keys[0].source_columns, ["order_id", "line"]);
    assert_eq!(notes.foreign_keys[0].target_columns, ["order_id", "line"]);

    assert!(users.indexes.iter().any(|i| i.method.as_deref() == Some("fulltext")));
    assert!(users.indexes.iter().any(|i| i.columns.is_empty() && !i.expressions.is_empty()));
    assert!(users.indexes.iter().any(|i| i.primary));
}

#[tokio::test]
async fn mysql_views_triggers_routines() {
    let Some(pool) = pool().await else { return };
    let s = mysql::introspect(&pool).await.unwrap();
    assert_eq!(s.views.len(), 2);
    assert_eq!(s.triggers.len(), 1);
    assert_eq!(s.triggers[0].timing, "BEFORE");
    assert!(s.routines.iter().any(|r| r.name == "tax"));
    let dep = |from: &str, to: &str| {
        s.dependencies.iter().any(|d| d.kind == DependencyKind::DependsOn && d.from.as_str().ends_with(from) && d.to.as_str().ends_with(to))
    };
    assert!(dep("order_totals", "shop_fixture.users"));
    assert!(dep("big_spenders", "view:shop_fixture.order_totals"));
    let again = mysql::introspect(&pool).await.unwrap();
    assert_eq!(s, again);
}
