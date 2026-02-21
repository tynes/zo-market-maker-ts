use serde::{Deserialize, Serialize};

/// Trading volume information for an account in a specific market.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountVolumeInfo {
    pub market_id: u32,
    pub volume_base: f64,
    pub volume_quote: f64,
}

/// Query parameters for fetching account volume data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountVolumeQuery {
    pub account_id: u32,
    pub market_id: Option<u32>,
    pub since: Option<String>,
    pub until: Option<String>,
}
