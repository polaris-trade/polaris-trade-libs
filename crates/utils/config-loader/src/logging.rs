use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct LoggerConfig {
    pub max_level: String,
    pub file: Option<FileLoggerConfig>,
    pub otel: Option<OtelConfig>,
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            max_level: "INFO".to_string(),
            file: None,
            otel: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct FileLoggerConfig {
    pub max_size: u64,
    pub path: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct OtelConfig {
    pub endpoint: String,
    pub enabled: bool,
}
