use serde::{Deserialize, Serialize};

use super::enums::{Side, TriggerKind, TriggerStatus};

/// Active trigger returned by `/account/{id}/triggers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInfo {
    pub account_id: u32,
    pub market_id: u32,
    pub trigger_price: u64,
    pub limit_price: Option<u64>,
    pub side: Side,
    pub kind: TriggerKind,
    pub action_id: u64,
    pub created_at: String,
}

/// Trigger history entry returned by `/account/{id}/triggers/history`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    pub account_id: u32,
    pub market_id: u32,
    pub trigger_price: u64,
    pub limit_price: Option<u64>,
    pub side: Side,
    pub kind: TriggerKind,
    pub status: TriggerStatus,
    pub created_at_action_id: u64,
    pub finalized_at_action_id: Option<u64>,
    pub created_at: String,
    pub finalized_at: String,
}
