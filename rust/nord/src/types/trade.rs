use serde::{Deserialize, Serialize};

use super::enums::Side;

/// A completed trade between a maker and taker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub time: String,
    pub action_id: u64,
    pub trade_id: u64,
    pub taker_id: u32,
    pub taker_side: Side,
    pub maker_id: u32,
    pub market_id: u32,
    pub order_id: u64,
    pub price: f64,
    pub base_size: f64,
}
