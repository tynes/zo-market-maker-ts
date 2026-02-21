use serde::{Deserialize, Serialize};

/// Full orderbook snapshot with ask/bid levels and summary statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderbookInfo {
    pub update_id: u64,
    /// Each entry is `[price, size]`.
    pub asks: Vec<[f64; 2]>,
    /// Each entry is `[price, size]`.
    pub bids: Vec<[f64; 2]>,
    pub asks_summary: SideSummary,
    pub bids_summary: SideSummary,
}

/// Aggregate statistics for one side (asks or bids) of the orderbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideSummary {
    pub sum: f64,
    pub count: u32,
}
