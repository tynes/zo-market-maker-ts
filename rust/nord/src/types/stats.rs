use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketStats {
    pub index_price: Option<f64>,
    pub index_price_conf: Option<f64>,
    pub frozen: Option<bool>,
    pub volume_base24h: f64,
    pub volume_quote24h: f64,
    pub high24h: Option<f64>,
    pub low24h: Option<f64>,
    pub close24h: Option<f64>,
    pub prev_close24h: Option<f64>,
    pub perp_stats: Option<PerpMarketStats>,
}

/// Note: PerpMarketStats uses snake_case in the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerpMarketStats {
    pub mark_price: Option<f64>,
    pub aggregated_funding_index: f64,
    pub funding_rate: f64,
    pub next_funding_time: String,
    pub open_interest: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStats {
    pub symbol: String,
    pub decimals: u8,
    pub mint_addr: String,
    pub weight_bps: u16,
    pub oracle_symbol: String,
    pub index_price: Option<TokenPrice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPrice {
    pub median: f64,
    pub confidence: f64,
}
