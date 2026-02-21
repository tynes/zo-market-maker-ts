use serde::{Deserialize, Serialize};

use crate::proto::nord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Ask,
    Bid,
}

impl From<Side> for nord::Side {
    fn from(s: Side) -> Self {
        match s {
            Side::Ask => nord::Side::Ask,
            Side::Bid => nord::Side::Bid,
        }
    }
}

impl From<nord::Side> for Side {
    fn from(s: nord::Side) -> Self {
        match s {
            nord::Side::Ask => Side::Ask,
            nord::Side::Bid => Side::Bid,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillMode {
    Limit,
    PostOnly,
    ImmediateOrCancel,
    FillOrKill,
}

impl FillMode {
    pub fn to_proto(self) -> nord::FillMode {
        match self {
            FillMode::Limit => nord::FillMode::Limit,
            FillMode::PostOnly => nord::FillMode::PostOnly,
            FillMode::ImmediateOrCancel => nord::FillMode::ImmediateOrCancel,
            FillMode::FillOrKill => nord::FillMode::FillOrKill,
        }
    }
}

impl From<nord::FillMode> for FillMode {
    fn from(f: nord::FillMode) -> Self {
        match f {
            nord::FillMode::Limit => FillMode::Limit,
            nord::FillMode::PostOnly => FillMode::PostOnly,
            nord::FillMode::ImmediateOrCancel => FillMode::ImmediateOrCancel,
            nord::FillMode::FillOrKill => FillMode::FillOrKill,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerKind {
    StopLoss,
    TakeProfit,
}

impl TriggerKind {
    pub fn to_proto(self) -> nord::TriggerKind {
        match self {
            TriggerKind::StopLoss => nord::TriggerKind::StopLoss,
            TriggerKind::TakeProfit => nord::TriggerKind::TakeProfit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerStatus {
    Active,
    Success,
    Removed,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandleResolution {
    #[serde(rename = "1")]
    OneMinute,
    #[serde(rename = "5")]
    FiveMinutes,
    #[serde(rename = "15")]
    FifteenMinutes,
    #[serde(rename = "30")]
    ThirtyMinutes,
    #[serde(rename = "60")]
    SixtyMinutes,
    #[serde(rename = "1D")]
    OneDay,
    #[serde(rename = "1W")]
    OneWeek,
    #[serde(rename = "1M")]
    OneMonth,
}

impl std::fmt::Display for CandleResolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CandleResolution::OneMinute => write!(f, "1"),
            CandleResolution::FiveMinutes => write!(f, "5"),
            CandleResolution::FifteenMinutes => write!(f, "15"),
            CandleResolution::ThirtyMinutes => write!(f, "30"),
            CandleResolution::SixtyMinutes => write!(f, "60"),
            CandleResolution::OneDay => write!(f, "1D"),
            CandleResolution::OneWeek => write!(f, "1W"),
            CandleResolution::OneMonth => write!(f, "1M"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FillRole {
    Maker,
    Taker,
}

impl std::fmt::Display for FillRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FillRole::Maker => write!(f, "maker"),
            FillRole::Taker => write!(f, "taker"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AclRole {
    FeeManager,
    MarketManager,
    Admin,
}

impl AclRole {
    pub fn mask(self) -> u32 {
        match self {
            AclRole::FeeManager => 1,
            AclRole::MarketManager => 2,
            AclRole::Admin => 0x8000_0000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementOrigin {
    User,
    Trigger,
    Liquidation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinalizationReason {
    Filled,
    Canceled,
    Taken,
}
