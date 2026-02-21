use serde::{Deserialize, Serialize};

pub type FeeTierId = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeTierConfig {
    pub maker_fee_ppm: u32,
    pub taker_fee_ppm: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountFeeTier {
    pub account_id: u32,
    pub fee_tier: FeeTierId,
}
