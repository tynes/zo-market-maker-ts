use serde::{Deserialize, Serialize};

/// A single action record from the action log.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionsItem {
    pub action_id: u64,
    pub physical_time: String,
    pub payload: String,
}
