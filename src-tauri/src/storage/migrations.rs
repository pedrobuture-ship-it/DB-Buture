//! Versioned migrations for the internal DB-BUTURE SQLite.
//! This database holds only app state: never passwords.

pub const MIGRATIONS: &[(i64, &str, &str)] = &[(
    1,
    "initial",
    r#"
CREATE TABLE saved_connections (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    engine        TEXT NOT NULL CHECK (engine IN ('postgres','mysql','sqlite')),
    host          TEXT,
    port          INTEGER,
    database_name TEXT,
    username      TEXT,
    ssl_mode      TEXT NOT NULL DEFAULT 'prefer',
    file_path     TEXT,
    secret_key    TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE snapshots (
    id            TEXT PRIMARY KEY,
    connection_id TEXT,
    name          TEXT NOT NULL,
    sha256        TEXT NOT NULL,
    payload       TEXT NOT NULL,
    created_at    TEXT NOT NULL
);
CREATE INDEX idx_snapshots_connection ON snapshots(connection_id);

CREATE TABLE graph_layouts (
    fingerprint TEXT NOT NULL,
    view_kind   TEXT NOT NULL,
    layout      TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (fingerprint, view_kind)
);

CREATE TABLE app_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE analysis_history (
    id            TEXT PRIMARY KEY,
    connection_id TEXT,
    kind          TEXT NOT NULL,
    summary       TEXT NOT NULL,
    created_at    TEXT NOT NULL
);
"#,
)];
