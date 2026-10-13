# Performance

Measured, not estimated. Every number below was produced on the machine and with the commands listed in
"How to reproduce". Nothing here is extrapolated.

**Machine:** Linux 7.2 (x86-64), release builds for Rust, production build for the UI, PostgreSQL 17 in Docker
on loopback. Synthetic schemas have 12 columns per table, 1–3 foreign keys per table (preferential attachment,
so a few tables become hubs), one index per table and one view per ten tables.

## 1. Rust analysis core

`cargo run --release --example bench_synthetic` — pure in-memory work on the normalized model.

| tables | columns | edges | json MB | graph | impact (hub) | audit | health | centrality | index analysis | hash | diff (self) |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 100 | 1200 | 228 | 0.4 | 0 ms | 0 ms | 0 ms | 0 ms | 1 ms | 0 ms | 6 ms | 3 ms |
| 500 | 6000 | 1164 | 2.1 | 0 ms | 1 ms | 2 ms | 0 ms | 19 ms | 1 ms | 36 ms | 21 ms |
| 1000 | 12000 | 2288 | 4.1 | 0 ms | 2 ms | 6 ms | 0 ms | 75 ms | 3 ms | 67 ms | 42 ms |
| 5000 | 60000 | 11447 | 20.6 | 2 ms | 16 ms | 59 ms | 0 ms | 2051 ms | 74 ms | 355 ms | 232 ms |

Betweenness centrality is the only super-linear operation (Brandes, O(V·E)). It runs once per introspection on a
blocking thread and is cached, so the UI never waits for it.

## 2. Real PostgreSQL introspection

`bench_pg` against databases created from the same synthetic shape (tables, PKs, FKs, indexes, views). Steady
state of three runs; the first run is about 30% slower (cold catalog cache).

| tables | views | columns | introspect + normalize | serialize | IPC payload |
|---:|---:|---:|---:|---:|---:|
| 1000 | 100 | 11 999 | 476 ms | 5 ms | 4.8 MB |
| 5000 | 500 | 59 999 | 1135 ms | 25 ms | 24.2 MB |

The catalog is read with a fixed number of set-based queries (nine), independent of the number of tables.

## 3. UI-side data structures

`npx vite-node scripts/bench-frontend.ts` — the pure functions the explorer, search and ER view are built on.

| tables | columns | schema index | tree (collapsed) | tree (filtered) | search entries | search query | ER edges | heatmap |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 100 | 1200 | 0.8 ms | 0.7 ms | 1.2 ms | 0.4 ms | 0.3 ms | 0.1 ms | 0.4 ms |
| 500 | 6000 | 2.8 ms | 0.7 ms | 2.8 ms | 0.8 ms | 1.0 ms | 0.2 ms | 0.3 ms |
| 1000 | 12000 | 9.0 ms | 1.6 ms | 4.7 ms | 0.6 ms | 1.4 ms | 0.4 ms | 0.4 ms |
| 5000 | 60000 | 25.1 ms | 8.1 ms | 20.4 ms | 5.8 ms | 5.9 ms | 1.8 ms | 1.6 ms |

## 4. Auto layout (ELK)

`node scripts/bench-layout.mjs` — in the main thread, to compare algorithms. In the app the layout runs in a Web
Worker, so these durations never block the interface.

| nodes | layered (full quality) | layered (fast settings) | compact (rect packing) |
|---:|---:|---:|---:|
| 1000 | 2371 ms | 564 ms | 269 ms |
| 5000 | 38 939 ms | 3520 ms | 1930 ms |

Finding that shaped the implementation: the default layered configuration needs **39 s for 5000 nodes**. Above
400 nodes the app now switches to the fast settings (about 11× quicker), and above 1500 tables the default mode is
the compact packing.

## 5. End to end in the UI

Headless Chromium driving the production UI against the real PostgreSQL databases above, through the release
dev bridge (`scripts/e2e`, HTTP instead of Tauri IPC). Times include the transport of the JSON payload.

| tables | connect + load schema + overview | ER diagram: first nodes visible | explorer filter | command palette search (all columns) |
|---:|---:|---:|---:|---:|
| 1000 | 923 ms | 1560 ms | 132 ms | 64 ms |
| 5000 | 2590 ms | 6129 ms | 416 ms | 97 ms |

Techniques that make this possible: set-based catalog queries, `Map` indexes instead of repeated scans, a
virtualized object tree and results grid (only visible rows are mounted), level of detail in the ER diagram (nodes
collapse to their title below 45% zoom and to bare blocks below 20%, edges are limited to the selection on schemas
above 1500 tables), layout in a Web Worker, CPU heavy analyses on blocking threads with cached results, and
cancellable introspection.

## Caveats

* The 5000-table ER diagram renders every node in the DOM when the whole diagram is in view (6 s). It is usable
  for orientation; the intended way to work with such a schema is Focus Mode (1–3 hops around a table).
* Numbers from the debug dev bridge are 3–5× slower and are not representative; they were discarded.
* The synthetic schemas contain no comments, partitions or exotic types, so real catalogs carry slightly more data.

## How to reproduce

```bash
cd src-tauri
cargo run --release --example bench_synthetic
DB_BUTURE_BENCH_PG=postgres://user:pw@127.0.0.1:54329/bench1000 cargo run --release --example bench_pg
cd ..
npx vite-node scripts/bench-frontend.ts
node scripts/bench-layout.mjs
```
