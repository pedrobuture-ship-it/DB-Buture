# Security review

Scope: Tauri configuration, IPC surface, SQL handling, filesystem, credentials, logging, exports and snapshots.
Method: code review of every command and every place that builds SQL, plus tests that attack the weak points.
Items marked **fixed** were found during this review and are covered by a test.

## Threat model

DB-BUTURE is pointed at databases you care about. The things it must never do: modify a database, leak a credential,
let a malicious *database* (hostile names, definitions, comments) run code or write files, or let the interface reach
beyond what it needs.

## Tauri

| Area | State |
|---|---|
| Capabilities | one window capability: `core:default` and `dialog:allow-open` (to pick a SQLite file). `dialog:allow-save` was **removed** (see Filesystem). No `fs`, `shell`, `http` or `process` plugin. Narrowing `core:default` further is possible but could not be verified without a webview driver, so it was left as is |
| CSP | `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`. `'unsafe-inline'` is for styles only (React inline styles, CodeMirror); no inline or remote script. The layout worker is a same-origin file |
| IPC surface | 37 typed commands, all in `src/commands`. No command takes a shell command, a file path to read, or SQL to run without the guard |
| Remote content | none loaded; no telemetry; no network access other than to the databases the user configures |

### Filesystem

* **fixed:** exports used a `save_export(path, ...)` command, so a compromised webview could have written an arbitrary
  `.json/.md/.svg/.png/.sql` file anywhere. It is replaced by `save_export_as(suggestedName, content)`: **Rust opens the
  native save dialog** and writes only to the path the user chose, with an extension allow-list. The interface can no
  longer choose a destination.
* SQLite files are opened read-only and must already exist (`create_if_missing(false)`).
* The only other file the app writes is its own internal database in the OS app-data directory.

## Database safety

| Concern | Mitigation |
|---|---|
| Modifying data | read-only sessions (`default_transaction_read_only=on`, `SET SESSION TRANSACTION READ ONLY`, SQLite opened `read_only`), plus the SQL guard as a second layer. Tested against real servers: `CREATE TABLE` and `DELETE` fail with a read-only error |
| SQL guard | one statement only; allow-list of statement kinds (`SELECT`, `EXPLAIN` without `ANALYZE`, `SHOW`); keyword deny-list scanned over tokens (catches `WITH x AS (DELETE ...)`, `SELECT ... INTO`, `FOR UPDATE`); function deny-list (`nextval`, `lo_import`, `pg_terminate_backend`, `dblink`, ...). Never the only barrier |
| **fixed:** read-only bypass through settings | `SELECT set_config('default_transaction_read_only','off',false)` was not blocked; `set_config`, notification/WAL/replication functions are now denied, and `SET`, `BEGIN`, `START TRANSACTION READ WRITE`, `PRAGMA` and `ATTACH` are rejected as statement kinds |
| Timeouts | `statement_timeout` 30 s and `idle_in_transaction_session_timeout` 60 s on PostgreSQL, `max_execution_time` on MySQL, per-query timeout in the app, 10 s connect timeout |
| Cancellation | real server-side cancel (`pg_cancel_backend`, `KILL QUERY`); the test checks the backend is gone |
| Result size | results capped (default 500, max 10 000 rows); scripts for import / migration capped at 8 MB |
| SQL injection | every catalog query is a static string with bound parameters; `PRAGMA` calls use table-valued functions with bound names; nothing user-controlled is concatenated into catalog SQL |
| Identifier injection | generated SQL (index suggestions) quotes every identifier and doubles embedded quotes; tested with `a"; DROP TABLE users; --` |
| Hostile metadata | names and definitions are treated as untrusted: ids escape separators, control and invisible formatting characters (**fixed:** bidirectional overrides and zero-width joiners could make two names look identical); React escapes text; the ER SVG export escapes markup (tested with `<script>`); Markdown export escapes table cells |
| Partial access | missing privileges on optional catalog sections become warnings, not failures (tested with a revoked privilege on a real server) |
| TLS | the connect form warns when a remote host uses SSL mode `disable` or `prefer`. The default stays `prefer` to keep local development working |

## Credentials

* Passwords are stored only in the OS keyring (`keyring` crate). The internal SQLite has **no password column** (asserted by
  a test); it holds a reference such as `db-buture.connection.<uuid>`.
* `SavedConnection.secret_key` is `skip_serializing`: not even the reference reaches the interface.
* A password typed in the form is sent once and cleared from component state at the moment of submit (checked by the
  e2e run: the field is empty right after "Test Connection").
* No silent plaintext fallback: when the keyring is unavailable the user gets an explanatory error.
* `ConnectionConfig` has no password field at all, so it cannot be logged, serialized or exported by accident. Error
  messages keep only the first line of driver errors; a test confirms a wrong password is never echoed.
* Exports, snapshots and the Markdown / JSON schema files are built from the schema model only: they contain neither
  credentials nor host names.

## Logging

`tracing` is used for lifecycle events (migrations applied, cancellation failures). Connection strings, configs and
passwords are never logged because no log call receives them.

## Residual risks

* The SQL guard is a heuristic. A new engine function with side effects would pass until it is added to the deny-list;
  the read-only session and a read-only database account are what actually protect the data.
* `core:default` is broader than strictly necessary.
* A hostile database can still make the UI *display* confusing names (the escaping above stops spoofing of ids and
  exports, not of rendered text).
* Installers are not code-signed.
* The OS keyring must be available. On a headless Linux server without a Secret Service the app cannot save passwords.
