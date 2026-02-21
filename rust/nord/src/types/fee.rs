use serde::{Deserialize, Serialize};

/// Unique identifier for a fee tier.
pub type FeeTierId = u32;

/// Fee configuration for a single tier (maker/taker fees in parts per million).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeTierConfig {
    pub maker_fee_ppm: u32,
    pub taker_fee_ppm: u32,
}

/// Mapping of an account to its assigned fee tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountFeeTier {
    pub account_id: u32,
    pub fee_tier: FeeTierId,
}
