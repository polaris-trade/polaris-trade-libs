use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct KafkaConfig {
    pub enabled: bool,
    pub client_id: String,
    pub servers: String,
}
