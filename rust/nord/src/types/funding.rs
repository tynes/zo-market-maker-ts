use serde::{Deserialize, Serialize};

/// Funding payment information for an account's position.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountFundingInfo {
    pub time: String,
    pub action_id: u64,
    pub market_id: u32,
    pub position_size: f64,
    pub funding_pnl: f64,
}
