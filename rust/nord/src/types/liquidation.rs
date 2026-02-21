use serde::{Deserialize, Serialize};

/// Detailed information about a liquidation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiquidationInfo {
    pub time: String,
    pub action_id: u64,
    pub liquidator_id: u32,
    pub liquidatee_id: u32,
    pub fee: f64,
    pub liquidation_kind: LiquidationKind,
    pub market_id: Option<u32>,
    pub token_id: Option<u32>,
    pub order_id: Option<u64>,
    pub order_price: Option<f64>,
    pub order_size: Option<f64>,
    pub order_quote: Option<f64>,
    pub pre_omf: f64,
    pub pre_mmf: f64,
    pub pre_imf: f64,
    pub pre_cmf: f64,
    pub pre_pon: f64,
    pub pre_mf: f64,
    pub pre_pn: f64,
    pub post_omf: f64,
    pub post_mmf: f64,
    pub post_imf: f64,
    pub post_cmf: f64,
    pub post_pon: f64,
    pub post_mf: f64,
    pub post_pn: f64,
}

/// Type of liquidation action taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiquidationKind {
    PlaceOrder,
    CancelOrder,
    Bankruptcy,
}
