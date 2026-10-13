//! Runs against a real PostgreSQL. Set DB_BUTURE_TEST_PG_URL to enable, e.g.
//! postgres://postgres:buture_test@127.0.0.1:54329/buture
//! (the fixture is loaded with a separate setup connection; introspection itself is read-only).

use db_buture_lib::database::postgres;
use db_buture_lib::metadata::{DependencyKind, ReferentialAction};
use sqlx::postgres::PgPoolOptions;

async fn pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("DB_BUTURE_TEST_PG_URL").ok()?;
    let pool = PgPoolOptions::new().max_connections(2).connect(&url).await.expect("connect");
    sqlx::raw_sql(include_str!("fixtures/postgres.sql")).execute(&pool).await.expect("fixture");
    Some(pool)
}

#[tokio::test]
async fn introspects_fixture() {
    let Some(pool) = pool().await else { return };
    let s = postgres::introspect(&pool, false).await.unwrap();

    assert!(s.server_version.is_some());
    assert!(s.namespaces.iter().all(|n| !n.is_system));
    assert!(s.namespaces.iter().any(|n| n.name == "shop"));

    let users = s.tables.iter().find(|t| t.name == "users").unwrap();
    assert_eq!(users.comment.as_deref(), Some("Registered customers"));
    assert_eq!(users.primary_key.as_ref().unwrap().columns, ["id"]);
    assert_eq!(users.columns[0].identity.as_deref(), Some("always"));
    let email = users.columns.iter().find(|c| c.name == "email").unwrap();
    assert_eq!(email.data_type, "varchar");
    assert_eq!(email.native_type, "character varying(255)");
    assert!(!email.nullable);
    assert_eq!(email.comment.as_deref(), Some("Login e-mail"));
    assert_eq!(users.columns.iter().find(|c| c.name == "tags").unwrap().data_type, "text[]");
    assert_eq!(users.unique_constraints.len(), 1);

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

    let events = s.tables.iter().find(|t| t.name == "events").unwrap();
    assert_eq!(events.engine_specific_metadata["partitioned"], true);
}

#[tokio::test]
async fn introspects_indexes() {
    let Some(pool) = pool().await else { return };
    let s = postgres::introspect(&pool, false).await.unwrap();
    let users = s.tables.iter().find(|t| t.name == "users").unwrap();
    let idx = |n: &str| users.indexes.iter().find(|i| i.name == n).unwrap_or_else(|| panic!("{n}"));

    assert_eq!(idx("idx_users_meta").method.as_deref(), Some("gin"));
    assert_eq!(idx("idx_users_created").method.as_deref(), Some("brin"));
    assert_eq!(idx("idx_users_name_hash").method.as_deref(), Some("hash"));
    let expr = idx("idx_users_lower_email");
    assert!(expr.columns.is_empty());
    assert_eq!(expr.expressions.len(), 1);
    assert!(expr.expressions[0].contains("lower"));
    assert!(expr.predicate.as_deref().unwrap().contains("name"));
    assert!(users.indexes.iter().any(|i| i.primary && i.columns == ["id"]));

    let items = s.tables.iter().find(|t| t.name == "order_items").unwrap();
    let cover = items.indexes.iter().find(|i| i.name == "idx_items_cover").unwrap();
    assert_eq!(cover.columns, ["order_id"]);
    assert_eq!(cover.included_columns, ["qty"]);
}

#[tokio::test]
async fn introspects_views_triggers_and_dependencies() {
    let Some(pool) = pool().await else { return };
    let s = postgres::introspect(&pool, false).await.unwrap();

    assert!(s.views.iter().any(|v| v.name == "daily_orders" && v.materialized));
    let totals = s.views.iter().find(|v| v.name == "order_totals").unwrap();
    assert!(totals.definition.as_deref().unwrap().to_lowercase().contains("sum"));
    assert_eq!(totals.columns.len(), 2);

    let dep = |from: &str, to: &str| {
        s.dependencies.iter().any(|d| d.kind == DependencyKind::DependsOn && d.from.as_str().ends_with(from) && d.to.as_str().ends_with(to))
    };
    assert!(dep("shop.order_totals", "shop.users"));
    assert!(dep("shop.order_totals", "shop.orders"));
    assert!(dep("shop.big_spenders", "shop.order_totals"));

    assert_eq!(s.triggers.len(), 1);
    let t = &s.triggers[0];
    assert_eq!(t.timing, "BEFORE");
    assert_eq!(t.events, ["INSERT", "UPDATE"]);
    assert!(t.function.is_some());

    assert!(s.routines.iter().any(|r| r.name == "touch_user"));
    assert!(s.routines.iter().any(|r| r.name == "noop_proc" && r.kind == db_buture_lib::metadata::RoutineKind::Procedure));
    assert!(s.sequences.iter().any(|q| q.name == "standalone_seq" && q.owned_by.is_none()));
    assert!(s.sequences.iter().any(|q| q.name == "orders_id_seq" && q.owned_by.is_some()));
}

#[tokio::test]
async fn introspection_is_deterministic_and_system_schemas_optional() {
    let Some(pool) = pool().await else { return };
    let a = postgres::introspect(&pool, false).await.unwrap();
    let b = postgres::introspect(&pool, false).await.unwrap();
    assert_eq!(a, b);
    let sys = postgres::introspect(&pool, true).await.unwrap();
    assert!(sys.namespaces.iter().any(|n| n.name == "pg_catalog" && n.is_system));
    assert!(sys.tables.len() > a.tables.len());
}
