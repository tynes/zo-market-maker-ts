use serde::{Deserialize, Serialize};

use super::enums::{FillMode, Side, PlacementOrigin, FinalizationReason};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub update_id: u64,
    pub orders: Vec<OpenOrder>,
    pub positions: Vec<PositionSummary>,
    pub balances: Vec<Balance>,
    pub margins: AccountMarginsView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOrder {
    pub order_id: u64,
    pub market_id: u32,
    pub side: Side,
    pub size: f64,
    pub price: f64,
    pub original_order_size: f64,
    pub client_order_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionSummary {
    pub market_id: u32,
    pub open_orders: u16,
    pub perp: Option<PerpPosition>,
    pub action_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerpPosition {
    pub base_size: f64,
    pub price: f64,
    pub updated_funding_rate_index: f64,
    pub funding_payment_pnl: f64,
    pub size_price_pnl: f64,
    pub is_long: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub token_id: u32,
    pub token: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountMarginsView {
    pub omf: f64,
    pub mf: f64,
    pub imf: f64,
    pub cmf: f64,
    pub mmf: f64,
    pub pon: f64,
    pub pn: f64,
    pub bankruptcy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderInfo {
    pub added_at: String,
    pub updated_at: String,
    pub trade_id: u64,
    pub trader_id: u32,
    pub market_id: u32,
    pub order_id: u64,
    pub side: Side,
    pub placed_size: f64,
    pub filled_size: Option<f64>,
    pub update_action_id: u64,
    pub is_reduce_only: bool,
    pub fill_mode: FillMode,
    pub placed_price: f64,
    pub original_size_limit: Option<f64>,
    pub original_price_limit: Option<f64>,
    pub placement_origin: PlacementOrigin,
    pub finalization_reason: Option<FinalizationReason>,
    pub market_symbol: String,
    pub token_symbol: String,
}
