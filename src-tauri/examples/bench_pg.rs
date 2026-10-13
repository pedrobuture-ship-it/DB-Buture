//! Times a real PostgreSQL introspection (query + normalize) and the JSON payload size.
//!
//!     DB_BUTURE_BENCH_PG=postgres://user:pw@127.0.0.1:54329/bench1000 cargo run --release --example bench_pg

use std::time::Instant;

use db_buture_lib::database::postgres;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() {
    let url = std::env::var("DB_BUTURE_BENCH_PG").expect("set DB_BUTURE_BENCH_PG");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");
    for run in 1..=3 {
        let t = Instant::now();
        let schema = postgres::introspect(&pool, false)
            .await
            .expect("introspect");
        let took = t.elapsed();
        let t = Instant::now();
        let json = serde_json::to_vec(&schema).unwrap();
        println!(
            "run {run}: {} tables, {} views, {} columns | introspect+normalize {:.0} ms | serialize {:.0} ms | payload {:.1} MB",
            schema.tables.len(),
            schema.views.len(),
            schema.tables.iter().map(|t| t.columns.len()).sum::<usize>(),
            took.as_secs_f64() * 1000.0,
            t.elapsed().as_secs_f64() * 1000.0,
            json.len() as f64 / 1_048_576.0
        );
    }
}
