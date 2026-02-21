use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositInfo {
    pub time: String,
    pub action_id: u64,
    pub account_id: u32,
    pub token_id: u32,
    pub amount: f64,
    pub balance: f64,
    pub event_index: u64,
}
