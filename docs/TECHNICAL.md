# Technical overview

## Layers

```text
React / TypeScript          UI only: rendering, navigation state, view-model helpers
        ↓  Tauri IPC (typed commands, camelCase JSON)
Rust commands               src-tauri/src/commands      thin: validate input, call the core
        ↓
Core                        analysis · graph · plan · importer · export · query
        ↑
Normalized schema model     src-tauri/src/metadata      DatabaseSchema, stable ids, type mapping
        ↑
DB adapters                 src-tauri/src/database      postgres · mysql · sqlite · session (read-only)
        ↓  SQLx
PostgreSQL / MySQL / SQLite
```

* `storage/` is DB-BUTURE's own SQLite (connections without passwords, snapshots, layouts, settings, history) with
  versioned migrations. `security/` holds the credential store (OS keyring) and the SQL guard.
* The **normalized model** (`metadata/model.rs`) is the only thing the interface ever sees. Raw catalog rows
  (`pg_class`, `information_schema` rows, `PRAGMA` output) never leave the Rust side. Engine particularities are kept in
  `engineSpecificMetadata` (partitioning, index validity, storage engine, `WITHOUT ROWID`, ...).
* **Stable ids** (`metadata/ids.rs`) are deterministic, independent of query order and escape separators, control
  characters and invisible formatting characters, so hostile metadata cannot forge or visually spoof an id. Snapshots,
  diffs, graph nodes and persisted layouts are all keyed by them.
* Introspection is deterministic: results are sorted and validated, and optional sections (triggers, routines, view
  dependencies...) degrade to a warning when the account lacks privileges instead of failing the whole read.

## Facts, heuristics, suggestions

Every analysis result carries an `Evidence` value: `observed_fact`, `heuristic`, `suggestion` or `unknown`
(`analysis/mod.rs`). The UI renders them differently and never presents an inference as a fact. Impact analysis only
sees database objects and says so; application risk of a migration is always `unknown`.

## Main libraries and what they are responsible for

| Library | Role |
|---|---|
| **Tauri 2** | desktop shell, IPC, native dialogs, packaging |
| **sqlx** (postgres, mysql, sqlite; rustls) | all database access: pooling, TLS, queries, the internal SQLite |
| **sqlparser** | SQL import (DDL to model), view lineage (relations and expressions), SQL guard, migration simulator |
| **petgraph** | dependency graph, traversal (dependents / dependencies / neighbourhood), cycle detection |
| **serde / serde_json** | the IPC contract and canonical JSON for snapshots and exports |
| **sha2** | snapshot SHA-256 over the canonical, statistics-free form of the schema |
| **keyring** | passwords in the OS credential store; the internal database only keeps a reference |
| **tokio** | async runtime, timeouts, cancellation, blocking threads for CPU heavy analyses |
| **thiserror / tracing** | typed errors with a stable `kind`; structured logging (never secrets) |
| **React 19 + TypeScript (strict)** | UI |
| **Vite, Tailwind CSS 4** | build and styling (tokens in `src/styles/index.css`) |
| **@xyflow/react** | rendering of the ER and dependency graphs; no graph logic |
| **elkjs** | auto layout, run in a Web Worker |
| **CodeMirror 6** (+ `@codemirror/lang-sql`) | SQL editor with schema-aware completion |
| **@tanstack/react-query** | async state for IPC calls (schema, analyses, snapshots) |
| **zustand** | local UI state only (current view, selection, palette) |
| **lucide-react** | icons |
| **sql-formatter** | the editor's Format action |
| **Vitest + Testing Library** | frontend tests |

`recharts` is not installed: no chart in the product needs it, and unused dependencies were avoided.

## Conventions

* TypeScript `strict`, no `any` (enforced by ESLint), path alias `@/`.
* Rust: typed errors (`AppError`), no `unwrap()` on production paths, queries parametrized, static catalog SQL.
* The frontend does not compute analyses. It builds view models (tree rows, search entries, heatmap values, SVG).

## Testing

| Suite | What it covers | Count |
|---|---|---|
| `cargo test --lib` | model, ids, normalizer, graph, impact, audit, indexes, diff, snapshots, lineage, migration, importer, plans, SQL guard, storage, exports | 133 |
| `tests/postgres_integration.rs` | real PostgreSQL 17: introspection, read-only enforcement, partial introspection under revoked privileges | 6 |
| `tests/mysql_integration.rs` | real MySQL 8.4 | 2 |
| `tests/sqlite_integration.rs` | real SQLite files, read-only writes rejected | 3 |
| `tests/query_integration.rs` | typed results, caps, timeout, **real cancellation** (backend verified stopped), EXPLAIN on all three | 5 |
| `npm test` | search, filters, tree, graph helpers, ER SVG, impact, diff, health, plan view, palette, explorer | 66 |
| `scripts/e2e` | headless Chromium driving the real UI against PostgreSQL, MySQL and SQLite through the dev bridge | 23 steps |

Integration tests that need a database are skipped when their environment variable is not set.

### Dev bridge and end to end harness

There is no webview driver for Tauri on Linux, so `src-tauri/examples/dev_bridge.rs` exposes the **same command
functions** over loopback HTTP (using Tauri's mock runtime), and `scripts/e2e/shim.js` routes `invoke` to it.
It is an example target, never part of the shipped app. It uses an in-memory credential store and returns "cancelled"
for native save dialogs.

```bash
cargo build --example dev_bridge        # from src-tauri
npm run build
node scripts/e2e/run.mjs ./e2e-out      # starts the bridge + Vite, drives Chromium, writes screenshots
```

## Packaging

`tauri.conf.json` bundles every target valid for the host. Verified on Linux x86-64: `deb`, `rpm` and `AppImage` were
built successfully. MSI, NSIS, `.app` and `.dmg` are configured but **have not been built or tested** (no Windows or
macOS machine was available), and no platform other than Linux has been run.

## CI

`.github/workflows/ci.yml` runs the frontend gates (`npm ci`, typecheck, lint, test, build) and the Rust gates
(`cargo fmt --check`, `cargo clippy`, `cargo test`), with PostgreSQL and MySQL service containers so the adapter
integration tests run for real.
