//! Synthetic schema benchmark for the Rust analysis core.
//!
//!     cargo run --release --example bench_synthetic
//!
//! Generates schemas of increasing size and times the operations the UI depends on.
//! Numbers are printed as a markdown table (see docs/PERFORMANCE.md).

use std::time::Instant;

use db_buture_lib::analysis::{audit, centrality, diff, impact, indexes, snapshot};
use db_buture_lib::graph::SchemaGraph;
use db_buture_lib::metadata::*;

fn schema(n: usize) -> DatabaseSchema {
    let e = Engine::Postgres;
    let mut s = DatabaseSchema::empty(e, "synthetic");
    // deterministic pseudo random
    let mut x: u64 = 0x9E3779B97F4A7C15;
    let mut rnd = |m: usize| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x % m as u64) as usize
    };
    for i in 0..n {
        let name = format!("table_{i:05}");
        let tid = StableId::table(e, "public", &name);
        let columns: Vec<Column> = (0..12)
            .map(|c| Column {
                stable_id: StableId::child(e, "column", "public", &name, &format!("c{c}")),
                name: format!("c{c}"),
                namespace: "public".into(),
                ordinal: c + 1,
                data_type: "text".into(),
                native_type: "text".into(),
                nullable: c > 0,
                default: None,
                generated: None,
                identity: None,
                comment: None,
                engine_specific_metadata: EngineMeta::new(),
            })
            .collect();
        let mut foreign_keys = vec![];
        if i > 0 {
            for k in 0..(1 + rnd(3)) {
                // preferential attachment to low numbered tables creates a few hubs
                let target = rnd(i).min(rnd(i));
                let tname = format!("table_{target:05}");
                foreign_keys.push(ForeignKey {
                    stable_id: StableId::child(e, "constraint", "public", &name, &format!("fk{k}")),
                    name: format!("fk{k}"),
                    namespace: "public".into(),
                    source_table: tid.clone(),
                    source_columns: vec![format!("c{}", k + 1)],
                    target_table: StableId::table(e, "public", &tname),
                    target_columns: vec!["c0".into()],
                    on_delete: ReferentialAction::NoAction,
                    on_update: ReferentialAction::NoAction,
                    engine_specific_metadata: EngineMeta::new(),
                });
            }
        }
        let indexes = vec![Index {
            stable_id: StableId::object(e, "index", "public", &format!("idx_{i}")),
            name: format!("idx_{i}"),
            namespace: "public".into(),
            table: tid.clone(),
            columns: vec!["c1".into()],
            unique: false,
            primary: false,
            method: Some("btree".into()),
            predicate: None,
            expressions: vec![],
            included_columns: vec![],
            definition: None,
            engine_specific_metadata: EngineMeta::new(),
        }];
        for fk in &foreign_keys {
            s.dependencies.push(Dependency {
                from: tid.clone(),
                to: fk.target_table.clone(),
                kind: DependencyKind::ForeignKey,
            });
        }
        s.tables.push(Table {
            stable_id: tid.clone(),
            name,
            namespace: "public".into(),
            comment: None,
            estimated_rows: Some(rnd(1_000_000) as i64),
            size_bytes: Some(rnd(1 << 30) as i64),
            columns,
            primary_key: None,
            foreign_keys,
            unique_constraints: vec![],
            check_constraints: vec![],
            indexes,
            engine_specific_metadata: EngineMeta::new(),
        });
    }
    for i in 0..n / 10 {
        let name = format!("view_{i:05}");
        let id = StableId::object(e, "view", "public", &name);
        for _ in 0..3 {
            s.dependencies.push(Dependency {
                from: id.clone(),
                to: StableId::table(e, "public", &format!("table_{:05}", rnd(n))),
                kind: DependencyKind::DependsOn,
            });
        }
        s.views.push(View {
            stable_id: id,
            name,
            namespace: "public".into(),
            materialized: false,
            definition: None,
            columns: vec![],
            comment: None,
            engine_specific_metadata: EngineMeta::new(),
        });
    }
    s.dependencies
        .sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
    s.dependencies.dedup();
    s
}

fn time<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed().as_secs_f64() * 1000.0)
}

fn main() {
    println!("| tables | columns | edges | json MB | graph | impact (hub) | audit | health | centrality | index analysis | hash | diff (self) |");
    println!("|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for n in [100usize, 500, 1000, 5000] {
        let s = schema(n);
        let columns: usize = s.tables.iter().map(|t| t.columns.len()).sum();
        let mb = serde_json::to_vec(&s).map(|v| v.len()).unwrap_or(0) as f64 / 1_048_576.0;
        let (g, t_graph) = time(|| SchemaGraph::build(&s));
        let hub = StableId::table(Engine::Postgres, "public", "table_00000");
        let (_, t_impact) = time(|| impact::analyze_impact(&s, &g, &hub));
        let (findings, t_audit) = time(|| audit::run_audit(&s, &g));
        let (_, t_health) = time(|| audit::health(&s, &findings));
        let (_, t_cent) = time(|| centrality::centrality(&s));
        let (_, t_idx) = time(|| indexes::suggestions(&s));
        let (_, t_hash) = time(|| snapshot::sha256_hex(&s));
        let (_, t_diff) = time(|| diff::diff_schemas(&s, &s));
        println!(
            "| {n} | {columns} | {} | {mb:.1} | {t_graph:.0} ms | {t_impact:.0} ms | {t_audit:.0} ms | {t_health:.0} ms | {t_cent:.0} ms | {t_idx:.0} ms | {t_hash:.0} ms | {t_diff:.0} ms |",
            s.dependencies.len()
        );
    }
}
