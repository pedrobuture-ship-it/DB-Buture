use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;

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
}

pub struct Active {
    pub session: Arc<Session>,
    pub name: String,
    pub connection_id: Option<String>,
    pub cache: Option<Arc<CachedSchema>>,
}

/// Introspection result plus derived structures, shared between commands.
pub struct CachedSchema {
    pub schema: DatabaseSchema,
    pub graph: SchemaGraph,
    pub fetched_at: String,
    pub include_system: bool,
    pub fingerprint: String,
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
}

impl AppState {
    pub fn new(storage: Storage, credentials: Arc<dyn CredentialStore>) -> Self {
        Self {
            storage,
            credentials,
            active: Mutex::new(None),
        }
    }

    pub async fn info(&self) -> Option<ActiveInfo> {
        let guard = self.active.lock().await;
        guard.as_ref().map(|a| ActiveInfo {
            connection_name: a.name.clone(),
            connection_id: a.connection_id.clone(),
            engine: a.session.config.engine,
            database: a.session.database_name(),
            server_version: a.session.server_version.clone(),
            read_only: true,
        })
    }

    pub async fn session(&self) -> AppResult<Arc<Session>> {
        let guard = self.active.lock().await;
        guard
            .as_ref()
            .map(|a| a.session.clone())
            .ok_or(AppError::NotConnected)
    }

    /// Returns the cached schema, introspecting when missing, requested, or when the system
    /// schema visibility changed.
    pub async fn schema(
        &self,
        refresh: bool,
        include_system: bool,
    ) -> AppResult<Arc<CachedSchema>> {
        let session = {
            let guard = self.active.lock().await;
            let active = guard.as_ref().ok_or(AppError::NotConnected)?;
            if !refresh {
                if let Some(c) = &active.cache {
                    if c.include_system == include_system {
                        return Ok(c.clone());
                    }
                }
            }
            active.session.clone()
        };

        let schema = session.introspect(include_system).await?;
        let cached = Arc::new(CachedSchema {
            graph: SchemaGraph::build(&schema),
            fingerprint: snapshot::sha256_hex(&schema),
            fetched_at: chrono::Utc::now().to_rfc3339(),
            include_system,
            schema,
        });
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
}
