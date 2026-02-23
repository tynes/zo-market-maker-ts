pub mod actions;
pub mod admin;
pub mod client;
pub mod config;
pub mod error;
pub mod orderbook;
pub mod proto;
pub mod rest;
pub mod types;
pub mod user;
pub mod utils;
pub mod ws;

#[cfg(feature = "solana")]
pub mod solana;

// ---- Top-level re-exports for ergonomic usage ----

// Client + user + admin
pub use admin::NordAdmin;
pub use client::Nord;
pub use config::NordConfig;
pub use error::{NordError, Result};
pub use user::NordUser;

// REST client
pub use rest::NordHttpClient;

// Core enums
pub use types::{
    AclRole, CandleResolution, FillMode, FillRole, FinalizationReason, LiquidationKind,
    PlacementOrigin, Side, TriggerKind, TriggerStatus,
};

// Market + token info
pub use types::{MarketInfo, MarketsInfo, TokenInfo};

// Account types
pub use types::{Account, AccountMarginsView, Balance, OpenOrder, PerpPosition, PositionSummary};

// Orderbook (REST snapshot types)
pub use types::{OrderbookInfo, SideSummary};

// Orderbook (live stream)
pub use orderbook::{MidPrice, OrderbookDepth, OrderbookSide, OrderbookStream, BBO};

// Trade + order history
pub use types::{OrderInfo, Trade};

// Stats
pub use types::{MarketStats, PerpMarketStats, TokenPrice, TokenStats};

// PnL + funding + volume
pub use types::{AccountFundingInfo, AccountPnlInfo, AccountVolumeInfo};

// Triggers
pub use types::{Trigger, TriggerInfo};

// Deposits + withdrawals
pub use types::{DepositInfo, WithdrawalInfo};

// Liquidations
pub use types::LiquidationInfo;

// Fees
pub use types::{AccountFeeTier, FeeTierConfig};

// Admin
pub use types::AdminInfo;

// Pagination
pub use types::{PageResult, PagedQuery};

// Actions / quote size
pub use types::{ActionsItem, QuoteSize};

// User info
pub use types::{User, UserSession};

// WebSocket events
pub use ws::events::{
    WebSocketAccountUpdate, WebSocketCandleUpdate, WebSocketDeltaUpdate, WebSocketMessage,
    WebSocketTradeUpdate,
};
