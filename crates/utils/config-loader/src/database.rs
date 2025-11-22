use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct MssqlConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub pool_size: Option<u32>,
    pub min_idle: Option<u32>,
    /// In seconds
    pub connection_timeout: Option<u64>,
}
