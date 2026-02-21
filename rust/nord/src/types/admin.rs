use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminInfo {
    pub key: String,
    pub roles: Vec<String>,
}
