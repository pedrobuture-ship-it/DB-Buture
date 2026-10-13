use serde::Serialize;

/// Typed application error returned across the IPC boundary.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorPayload {
    kind: &'static str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let kind = match self {
            AppError::Internal(_) => "internal",
        };
        ErrorPayload { kind, message: self.to_string() }.serialize(s)
    }
}

pub type AppResult<T> = Result<T, AppError>;
