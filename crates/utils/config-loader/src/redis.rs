use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct RedisConfig {
    pub mode: RedisMode,
    pub host: String,
    pub port: u16,
    pub database: Option<u8>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum RedisMode {
    Single,
    Sentinel,
    Cluster,
}
