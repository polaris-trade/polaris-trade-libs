use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Env {
    #[serde(rename = "dev")]
    Development,
    Staging,
    Production,
    Unknown(String),
}

impl From<String> for Env {
    fn from(value: String) -> Self {
        match value.to_lowercase().as_str() {
            "development" | "dev" | "sit" => Env::Development,
            "staging" | "stg" => Env::Staging,
            "production" | "prod" => Env::Production,
            other => Env::Unknown(other.to_string()),
        }
    }
}
