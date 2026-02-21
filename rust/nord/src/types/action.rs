use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionsItem {
    pub action_id: u64,
    pub physical_time: String,
    pub payload: String,
}
