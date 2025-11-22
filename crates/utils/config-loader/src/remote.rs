use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct RemoteConfig {
    pub config: _RemoteConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct _RemoteConfig {
    pub url: String,
}
