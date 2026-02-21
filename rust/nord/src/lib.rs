pub mod proto;
pub mod error;
pub mod config;
pub mod types;
pub mod utils;
pub mod rest;
pub mod actions;
pub mod client;
pub mod ws;
pub mod user;
pub mod admin;

#[cfg(feature = "solana")]
pub mod solana;

// ---- Top-level re-exports for ergonomic usage ----

// Client + user + admin
pub use client::Nord;
pub use user::NordUser;
pub use admin::NordAdmin;
pub use error::{NordError, Result};
pub use config::NordConfig;

// REST client
pub use rest::NordHttpClient;

// Core enums
pub use types::{
    Side, FillMode, FillRole, TriggerKind, TriggerStatus,
    CandleResolution, PlacementOrigin, FinalizationReason,
    AclRole, LiquidationKind,
};

// Market + token info
pub use types::{MarketsInfo, MarketInfo, TokenInfo};

// Account types
pub use types::{Account, OpenOrder, PositionSummary, PerpPosition, Balance, AccountMarginsView};

// Orderbook
pub use types::{OrderbookInfo, SideSummary};

// Trade + order history
pub use types::{Trade, OrderInfo};

// Stats
pub use types::{MarketStats, PerpMarketStats, TokenStats, TokenPrice};

// PnL + funding + volume
pub use types::{AccountPnlInfo, AccountFundingInfo, AccountVolumeInfo};

// Triggers
pub use types::{TriggerInfo, Trigger};

// Deposits + withdrawals
pub use types::{DepositInfo, WithdrawalInfo};

// Liquidations
pub use types::LiquidationInfo;

// Fees
pub use types::{FeeTierConfig, AccountFeeTier};

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
    WebSocketMessage, WebSocketTradeUpdate, WebSocketDeltaUpdate,
    WebSocketAccountUpdate, WebSocketCandleUpdate,
};
