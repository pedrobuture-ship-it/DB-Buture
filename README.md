# DB-BUTURE

> Visual Database Intelligence for Developers.

**See your database. Understand the consequences.**

DB-BUTURE is a local-first desktop tool that connects to a real database (or reads a `.sql` script) and turns its
structure into something you can look at: relationships, dependencies, the blast radius of a change, where a view's
columns really come from, how the schema drifted between two points in time, and how the engine plans to run a query.

> **DB-BUTURE is read-only by design.** It observes, explains and simulates. It does not secretly modify.

| Engine | Depth |
|---|---|
| PostgreSQL | excellent: tables, columns, composite keys, all index methods, views and materialized views with dependencies, triggers, sequences, routines |
| MySQL | solid: `information_schema` (tables, columns, keys, checks, indexes, views, triggers, routines) for the connected database |
| SQLite | solid: `sqlite_schema` + `PRAGMA`, opened read-only |

## What you can do

| | |
|---|---|
| **ER Diagram** | pan, zoom, minimap, drag, persisted layouts, auto layout (hierarchical / compact / horizontal / vertical), Focus Mode (1–3 hops), relationship highlighting, heatmaps (size, rows, relationship density, risk), shortest path between tables, SVG / PNG export |
| **Impact Analysis** | which objects depend on a table or column, split into **DIRECT** and **TRANSITIVE**, with every item marked as observed fact / heuristic / unknown. It never claims that external code will break |
| **Data Lineage** | table and column lineage for views, combining catalog dependencies with a SQL parser; columns are classified `DIRECT` / `DERIVED` / `AGGREGATED` / `UNKNOWN` and the result says when confidence is only partial |
| **Health & Audit** | an explainable score: every deducted point traces back to a rule and an object; facts and heuristics are kept apart |
| **Index Analysis** | foreign keys without a supporting index, duplicate / overlapping indexes, and copy-only `CREATE INDEX` suggestions |
| **Query Plans** | `EXPLAIN` (never `ANALYZE`) for the three engines, rendered as a cost-annotated tree with labelled heuristic observations |
| **SQL Editor** | CodeMirror 6 with schema-aware autocomplete, result caps, timeouts, cancellation. Mutating SQL is rejected and the connection itself is read-only |
| **Snapshots & Schema Diff** | deterministic structural snapshots with SHA-256, history, compare, export; diff matched by stable ids |
| **Environments** | compare the structure of two saved connections (staging vs production). Data is never read |
| **Migration simulator** | apply DDL to a *copy* of the model and see statement results, the resulting diff and structural / runtime / application risk. Nothing is executed |
| **Import SQL** | no database needed: paste or open a `.sql` file; anything that is not understood is listed verbatim (`PARSED WITH WARNINGS`) |
| **Search** | `Ctrl + K` over tables, columns, views, indexes and constraints |

## Screenshots

Real screens captured from the running UI against real PostgreSQL / MySQL / SQLite databases.

| | |
|---|---|
| ![Connect](docs/screenshots/connect.png) | ![Overview](docs/screenshots/overview.png) |
| ![ER diagram](docs/screenshots/er-diagram.png) | ![Heatmap and path finder](docs/screenshots/heatmap-path.png) |
| ![Impact analysis](docs/screenshots/impact.png) | ![Health](docs/screenshots/health.png) |
| ![Query plan](docs/screenshots/query-plan.png) | ![Migration simulator](docs/screenshots/migration.png) |
| ![Lineage](docs/screenshots/lineage.png) | ![Import SQL](docs/screenshots/import-sql.png) |

## Architecture

```text
React
  ↓
Tauri
  ↓
Rust
  ↓
SQLx
  ↓
Databases
```

The interface never talks to a database. Rust reads the catalog, normalizes it into an engine independent model, and
runs every analysis (graph traversal, impact, lineage, audit, diff, plans). React only renders. See
[docs/TECHNICAL.md](docs/TECHNICAL.md) for the layers and the role of each main library.

## Safety

* Sessions are opened read-only (PostgreSQL `default_transaction_read_only`, MySQL `SET SESSION TRANSACTION READ ONLY`,
  SQLite read-only file), with statement timeouts. Mutating SQL is also rejected before it is sent.
* **The strongest protection is still a database account that only has read privileges.** Point DB-BUTURE at production
  with such an account.
* Passwords live only in the operating system keyring. They are never written to the internal database, never sent
  back to the interface and never exported. If no keyring is available the app says so; there is no plaintext fallback.
* Exports and snapshots contain schema structure only.

Details and the audit are in [docs/SECURITY.md](docs/SECURITY.md).

## Getting started

Requirements: Node.js, Rust (stable) and the Tauri 2 system dependencies for your platform
(on Linux: WebKitGTK 4.1, libsoup 3 and a Secret Service keyring such as gnome-keyring or KWallet).

```bash
npm install
npm run tauri dev        # run the desktop app
npm run tauri build      # build installers for the current platform
```

Quality gates:

```bash
npm run check            # typecheck + lint + frontend tests
cd src-tauri
cargo fmt --check && cargo clippy --all-targets && cargo test
```

The adapter integration tests run against real databases when these variables are set (they are skipped otherwise):

```bash
export DB_BUTURE_TEST_PG_URL=postgres://postgres:buture_test@127.0.0.1:54329/buture
export DB_BUTURE_TEST_MYSQL_URL=mysql://root:buture_test@127.0.0.1:33069/shop
docker run -d --rm -p 127.0.0.1:54329:5432 -e POSTGRES_PASSWORD=buture_test -e POSTGRES_DB=buture postgres:17
docker run -d --rm -p 127.0.0.1:33069:3306 -e MYSQL_ROOT_PASSWORD=buture_test -e MYSQL_DATABASE=shop mysql:8.4
cargo test -- --test-threads=1
```

## Documentation

* [docs/TECHNICAL.md](docs/TECHNICAL.md): architecture, libraries, conventions, testing, dev tooling
* [docs/SECURITY.md](docs/SECURITY.md): Tauri and database security review
* [docs/PERFORMANCE.md](docs/PERFORMANCE.md): measured numbers for 100 to 5000 tables
* [docs/RELEASE_AUDIT.md](docs/RELEASE_AUDIT.md): what was validated, how, and what was not

---

Copyright © Pedro Buture. All rights reserved.
