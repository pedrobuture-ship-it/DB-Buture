use serde::Serialize;

/// Typed application error returned across the IPC boundary.
///
/// Messages must never contain passwords or connection strings with secrets.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("connection failed: {0}")]
    Connection(String),
    #[error("not connected")]
    NotConnected,
    #[error("storage error: {0}")]
    Storage(String),
    #[error("credential store unavailable: {0}")]
    CredentialStore(String),
    #[error("blocked by read-only mode: {0}")]
    ReadOnly(String),
    #[error("operation timed out after {0} ms")]
    Timeout(u64),
    #[error("operation cancelled")]
    Cancelled,
    #[error("introspection failed: {0}")]
    Introspection(String),
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "validation",
            AppError::Connection(_) => "connection",
            AppError::NotConnected => "not_connected",
            AppError::Storage(_) => "storage",
            AppError::CredentialStore(_) => "credential_store",
            AppError::ReadOnly(_) => "read_only",
            AppError::Timeout(_) => "timeout",
            AppError::Cancelled => "cancelled",
            AppError::Introspection(_) => "introspection",
            AppError::Internal(_) => "internal",
        }
    }
}

#[derive(Serialize)]
struct ErrorPayload {
    kind: &'static str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ErrorPayload { kind: self.kind(), message: self.to_string() }.serialize(s)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        // sqlx errors for connection failures never embed the password, but we only
        // forward the display text, never the URL/options.
        match err {
            sqlx::Error::PoolTimedOut => AppError::Timeout(0),
            other => AppError::Storage(other.to_string()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_kind_and_message() {
        let json = serde_json::to_value(AppError::Validation("bad port".into())).unwrap();
        assert_eq!(json["kind"], "validation");
        assert_eq!(json["message"], "bad port");
    }
}
