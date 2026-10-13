use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{Mutex, Notify};

use crate::analysis::snapshot;
use crate::database::Session;
use crate::error::{AppError, AppResult};
use crate::graph::SchemaGraph;
use crate::metadata::{DatabaseSchema, Engine};
use crate::security::CredentialStore;
use crate::storage::Storage;

pub struct AppState {
    pub storage: Storage,
    pub credentials: Arc<dyn CredentialStore>,
    pub active: Mutex<Option<Active>>,
    /// Cancellation handles of statements that are currently running, by client supplied id.
    pub running: Mutex<HashMap<String, Arc<Notify>>>,
}

pub struct Active {
    /// `None` for a workspace opened from imported SQL: there is no database to talk to.
    pub session: Option<Arc<Session>>,
    pub name: String,
    pub connection_id: Option<String>,
    pub cache: Option<Arc<CachedSchema>>,
    /// Stable, secret-free identity used to persist layouts.
    pub layout_key: String,
}

/// Introspection result plus derived structures, shared between commands.
pub struct CachedSchema {
    pub schema: DatabaseSchema,
    pub graph: SchemaGraph,
    pub fetched_at: String,
    pub include_system: bool,
    pub fingerprint: String,
}

impl CachedSchema {
    pub fn new(schema: DatabaseSchema, include_system: bool) -> Self {
        Self {
            graph: SchemaGraph::build(&schema),
            fingerprint: snapshot::sha256_hex(&schema),
            fetched_at: chrono::Utc::now().to_rfc3339(),
            include_system,
            schema,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveInfo {
    pub connection_name: String,
    pub connection_id: Option<String>,
    pub engine: Engine,
    pub database: String,
    pub server_version: String,
    pub read_only: bool,
    /// True when the workspace comes from imported SQL rather than a live database.
    pub imported: bool,
}

impl AppState {
    pub fn new(storage: Storage, credentials: Arc<dyn CredentialStore>) -> Self {
        Self {
            storage,
            credentials,
            active: Mutex::new(None),
            running: Mutex::new(HashMap::new()),
        }
    }

    pub async fn info(&self) -> Option<ActiveInfo> {
        let guard = self.active.lock().await;
        guard.as_ref().map(|a| match &a.session {
            Some(s) => ActiveInfo {
                connection_name: a.name.clone(),
                connection_id: a.connection_id.clone(),
                engine: s.config.engine,
                database: s.database_name(),
                server_version: s.server_version.clone(),
                read_only: true,
                imported: false,
            },
            None => {
                let schema = a.cache.as_ref().map(|c| &c.schema);
                ActiveInfo {
                    connection_name: a.name.clone(),
                    connection_id: None,
                    engine: schema.map(|s| s.engine).unwrap_or(Engine::Generic),
                    database: schema.map(|s| s.database.clone()).unwrap_or_default(),
                    server_version: "imported SQL".into(),
                    read_only: true,
                    imported: true,
                }
            }
        })
    }

    /// The live session. Fails with a clear message for imported (offline) workspaces.
    pub async fn session(&self) -> AppResult<Arc<Session>> {
        let guard = self.active.lock().await;
        let active = guard.as_ref().ok_or(AppError::NotConnected)?;
        active.session.clone().ok_or_else(|| {
            AppError::Validation(
                "this workspace was imported from SQL and has no database connection".into(),
            )
        })
    }

    pub async fn layout_key(&self) -> AppResult<String> {
        let guard = self.active.lock().await;
        Ok(guard
            .as_ref()
            .ok_or(AppError::NotConnected)?
            .layout_key
            .clone())
    }

    /// Returns the cached schema, introspecting when missing, requested, or when the system
    /// schema visibility changed. Imported workspaces always return what was imported.
    pub async fn schema(
        &self,
        refresh: bool,
        include_system: bool,
    ) -> AppResult<Arc<CachedSchema>> {
        let session = {
            let guard = self.active.lock().await;
            let active = guard.as_ref().ok_or(AppError::NotConnected)?;
            if active.session.is_none() {
                return active.cache.clone().ok_or(AppError::NotConnected);
            }
            if !refresh {
                if let Some(c) = &active.cache {
                    if c.include_system == include_system {
                        return Ok(c.clone());
                    }
                }
            }
            active.session.clone()
        };
        let session = session.ok_or(AppError::NotConnected)?;
        let cached = Arc::new(CachedSchema::new(
            session.introspect(include_system).await?,
            include_system,
        ));
        if let Some(active) = self.active.lock().await.as_mut() {
            active.cache = Some(cached.clone());
        }
        Ok(cached)
    }

    /// The cached schema as last loaded (does not hit the database).
    pub async fn current(&self) -> AppResult<Arc<CachedSchema>> {
        let guard = self.active.lock().await;
        guard
            .as_ref()
            .ok_or(AppError::NotConnected)?
            .cache
            .clone()
            .ok_or_else(|| AppError::Validation("schema not loaded yet".into()))
    }

    pub async fn replace_active(&self, next: Active) {
        let mut guard = self.active.lock().await;
        if let Some(old) = guard.take() {
            if let Some(s) = old.session {
                s.close().await;
            }
        }
        *guard = Some(next);
    }
}
