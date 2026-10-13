# Release audit

Version under audit: **0.1.0**. This is a pre-release audit: DB-BUTURE is **not declared 1.0**. Several checklist items
below could not be completed in this environment and are listed as such, not glossed over.

Environment: Linux x86-64; PostgreSQL 17.11 and MySQL 8.4.11 in Docker; SQLite 3.51 files; Chromium for UI runs.

## Feature validation against real databases

"Real" means a running server or file, not a mock. Evidence is the test that exercises it.

| Area | PostgreSQL | MySQL | SQLite | Evidence |
|---|:-:|:-:|:-:|---|
| Connection (test, connect, latency, version) | ✅ | ✅ | ✅ | `session_is_read_only_and_times_out`, e2e "connect" steps |
| Introspection (tables, columns, composite PK/FK/UNIQUE, checks, indexes, views, triggers) | ✅ | ✅ | ✅ | `introspects_fixture`, `introspects_mysql`, `introspects_sqlite_file` |
| Index methods (btree, gin, brin, hash, expression, partial, INCLUDE) | ✅ | fulltext, expression | partial, expression | `introspects_indexes` |
| Materialized views, sequences, routines | ✅ | routines | n/a | `introspects_views_triggers_and_dependencies` |
| View dependency edges | ✅ catalog | ✅ `VIEW_TABLE_USAGE` | heuristic text match | same, plus lineage tests |
| ER diagram, Focus, layout, search | ✅ | ✅ | ✅ | e2e "er diagram", "mysql connect", "palette" |
| Impact analysis | ✅ | – | – | e2e "impact" (unit-tested on the model for all engines) |
| Lineage | ✅ | – | – | e2e "lineage"; 9 parser tests |
| Audit / health / index analysis | ✅ | – | – | e2e "health", "index analysis" |
| SQL editor, results, caps | ✅ | ✅ | ✅ | `postgres_runs_typed_queries_and_blocks_writes`, `mysql_runs_queries_and_explains`, `sqlite_runs_queries_and_explains` |
| EXPLAIN (no ANALYZE) | ✅ | ✅ | ✅ | `postgres_explain_is_estimate_only` and siblings |
| Snapshots, diff | ✅ | – | – | `snapshot_lifecycle`, e2e "snapshot + identical diff" |
| Environment compare | unit only | unit only | unit only | the diff engine is tested; the two-connection path was **not** exercised end to end |
| Migration simulator | model level | model level | model level | 13 tests; e2e "migration simulator" |
| Export JSON / Markdown / SVG | ✅ | ✅ | ✅ | exporter and SVG tests. **PNG export and the native save dialog were not exercised in a real window** |

## Security validation

| Check | Result |
|---|---|
| No plaintext password anywhere | ✅ no password column (test), no password in config types, field cleared on submit (e2e) |
| Read-only enforcement | ✅ real servers reject `CREATE`/`DELETE` through the session; guard blocks the rest |
| Timeouts | ✅ `postgres_timeout_and_cancel` |
| Cancellation | ✅ same test verifies the server-side statement stopped |
| No secret export | ✅ exports derive from the model only; test scans for `password` / `secret` |
| Tauri / IPC / filesystem / SQL review | ✅ [SECURITY.md](SECURITY.md), four real issues found and fixed |

## Performance

Measured for 100 / 500 / 1000 / 5000 tables in [PERFORMANCE.md](PERFORMANCE.md). 1000 tables: 0.9 s to connect and load,
1.6 s to the first ER nodes. 5000 tables: 2.6 s to load, 6.1 s to the ER diagram, usable but intended to be explored
through Focus Mode. A layout that took 39 s was found and fixed (worker + fast settings).

## Build and packaging

| Target | Status |
|---|---|
| Linux `deb`, `rpm`, `AppImage` | ✅ built (12.6 / 12.6 / 106.9 MiB) |
| Release binary starts, opens its window, applies internal migrations | ✅ observed |
| Windows `msi` / `nsis`, macOS `.app` / `.dmg` | ⚠️ configured, **not built, not tested** |
| Real window screenshot | ⚠️ not obtained (the compositor screenshot tool hung); UI evidence comes from headless Chromium running the real frontend against the real Rust command layer |
| Code signing | ❌ none |

## Known limitations

* MySQL introspects the connected database only; foreign keys into other databases are reported as warnings.
* PostgreSQL: no domains, custom types, extensions, row-level policies or partition hierarchy yet.
* SQLite has no dependency catalog; view dependencies are a text match and are labelled accordingly.
* Lineage handles `SELECT` / `UNION` bodies; CTEs and subqueries are reported as `UNKNOWN`, never guessed.
* The migration simulator models common DDL (create/drop/alter table, columns, constraints, indexes, views); anything else is reported as ignored.
* Environment comparison, PNG export and keyring access were not verified on a second machine or in CI.

## Final architecture audit

1. **Does React only hold UI?** Yes. It never reaches a database; all analysis (graph, impact, lineage, audit, diff,
   paths, centrality, migration, plans) is Rust. React builds view models (tree rows, search entries, heatmap values,
   SVG) from the normalized model.
2. **Are adapters isolated?** Yes: `database/{postgres,mysql,sqlite}` produce the normalized model and nothing outside
   sees catalog rows. `session.rs` and `query/` contain the engine-specific connection and execution code.
3. **Does the normalized schema represent the engines?** For tables, keys, indexes, views, triggers, routines and
   sequences, yes, keeping engine details in `engineSpecificMetadata`. The gaps are listed above.
4. **Is petgraph responsible for the analyses?** For traversal, dependents, neighbourhood, paths and cycles, yes.
   Betweenness centrality is a self-contained Brandes implementation over the same FK structure, not petgraph.
5. **Is XYFlow only visualization?** Yes. Layout is ELK, focus and path queries are answered by Rust.
6. **Are credentials only in Rust / keyring?** Yes; a typed password crosses the IPC once and is cleared.
7. **Does the internal SQLite contain a secret?** No; only the keyring reference (and it is not serialized to the UI).
8. **Is it useful offline?** Yes: Import SQL opens a full offline workspace (ER, dependencies, impact, lineage, health,
   index analysis, snapshots, diff, migration simulation). SQL Editor, plans and refresh need a live connection and say so.
9. **Is it safe against production with a read-only user?** Yes: read-only sessions, statement timeouts, capped results,
   a guard, and only catalog reads in the background. A read-only account is still the recommended primary control.
10. **Is a 1000-table schema usable?** Yes, measured (see Performance).
11. **Could SQL Server / Oracle be added without rewriting the core?** The model, graph, analyses, diff, snapshots,
    exports and UI are engine neutral. A new engine needs an adapter, a session/pool variant, a parser dialect, type
    mapping and a plan parser; a handful of `match engine` sites (`session`, `query`, `sql_guard`, `types`) must learn the
    new variant. It is additive, not a rewrite.
12. **Is any open source licence attributed to DB-BUTURE?** No. There is no `LICENSE` file, no licence field in
    `package.json` or `Cargo.toml`, no badge and no licence text in the README. A repository scan for MIT / Apache / GPL /
    "open source" / "free software" / LICENSE finds nothing in the project's own files. Dependencies keep their own
    licences untouched.

Copyright © Pedro Buture. All rights reserved.
