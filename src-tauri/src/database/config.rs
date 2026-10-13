use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::metadata::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SslMode {
    Disable,
    #[default]
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl SslMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SslMode::Disable => "disable",
            SslMode::Prefer => "prefer",
            SslMode::Require => "require",
            SslMode::VerifyCa => "verifyca",
            SslMode::VerifyFull => "verifyfull",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "disable" => Some(SslMode::Disable),
            "prefer" => Some(SslMode::Prefer),
            "require" => Some(SslMode::Require),
            "verifyca" => Some(SslMode::VerifyCa),
            "verifyfull" => Some(SslMode::VerifyFull),
            _ => None,
        }
    }
}

/// Everything needed to reach a database except the password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub engine: Engine,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub username: Option<String>,
    #[serde(default)]
    pub ssl_mode: SslMode,
    /// SQLite only.
    pub file_path: Option<String>,
}

impl ConnectionConfig {
    pub fn default_port(engine: Engine) -> Option<u16> {
        match engine {
            Engine::Postgres => Some(5432),
            Engine::Mysql => Some(3306),
            _ => None,
        }
    }

    pub fn validate(&self) -> AppResult<()> {
        match self.engine {
            Engine::Postgres | Engine::Mysql => {
                require(&self.host, "host")?;
                require(&self.database, "database")?;
                require(&self.username, "username")?;
                if self.port == Some(0) {
                    return Err(AppError::Validation(
                        "port must be between 1 and 65535".into(),
                    ));
                }
            }
            Engine::Sqlite => require(&self.file_path, "file path")?,
            Engine::Generic => {
                return Err(AppError::Validation(
                    "generic is not a connectable engine".into(),
                ))
            }
        }
        Ok(())
    }
}

fn require(value: &Option<String>, field: &str) -> AppResult<()> {
    match value.as_deref().map(str::trim) {
        Some(v) if !v.is_empty() => Ok(()),
        _ => Err(AppError::Validation(format!("{field} is required"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg() -> ConnectionConfig {
        ConnectionConfig {
            engine: Engine::Postgres,
            host: Some("localhost".into()),
            port: Some(5432),
            database: Some("app".into()),
            username: Some("ro".into()),
            ssl_mode: SslMode::Prefer,
            file_path: None,
        }
    }

    #[test]
    fn valid_postgres_config() {
        assert!(pg().validate().is_ok());
    }

    #[test]
    fn missing_host_is_rejected() {
        let mut c = pg();
        c.host = Some("  ".into());
        assert!(matches!(c.validate(), Err(AppError::Validation(_))));
    }

    #[test]
    fn sqlite_needs_file_path() {
        let c = ConnectionConfig {
            engine: Engine::Sqlite,
            host: None,
            port: None,
            database: None,
            username: None,
            ssl_mode: SslMode::Disable,
            file_path: None,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn ssl_mode_roundtrips() {
        for m in [
            SslMode::Disable,
            SslMode::Prefer,
            SslMode::Require,
            SslMode::VerifyCa,
            SslMode::VerifyFull,
        ] {
            assert_eq!(SslMode::parse(m.as_str()), Some(m));
        }
    }
}
