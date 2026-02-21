use serde::{Deserialize, Serialize};

/// Exchange-wide markets and tokens configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketsInfo {
    pub markets: Vec<MarketInfo>,
    pub tokens: Vec<TokenInfo>,
}

/// Configuration for a single perpetual market.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketInfo {
    pub market_id: u32,
    pub symbol: String,
    pub price_decimals: u8,
    pub size_decimals: u8,
    pub base_token_id: u32,
    pub quote_token_id: u32,
    pub imf: f64,
    pub mmf: f64,
    pub cmf: f64,
}

/// Configuration for a single token (collateral asset).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    pub token_id: u32,
    pub symbol: String,
    pub decimals: u8,
    pub mint_addr: String,
    pub weight_bps: u16,
}
