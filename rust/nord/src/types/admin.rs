use serde::{Deserialize, Serialize};

/// Information about an admin user and their assigned roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminInfo {
    pub key: String,
    pub roles: Vec<String>,
}
