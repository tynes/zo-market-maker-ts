use serde::{Deserialize, Serialize};

/// Profit-and-loss information for an account in a specific market.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPnlInfo {
    pub time: String,
    pub action_id: u64,
    pub market_id: u32,
    pub trading_pnl: f64,
    pub settled_funding_pnl: f64,
    pub position_size: f64,
}
