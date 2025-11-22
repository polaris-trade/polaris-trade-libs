use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct BaseAppConfig {
    pub name: String,
    pub version: Option<String>,
    pub env: Option<String>,
    /// Timezone offset in hours from UTC (e.g., 7 for UTC+7)
    pub timezone: Option<i8>,
}
