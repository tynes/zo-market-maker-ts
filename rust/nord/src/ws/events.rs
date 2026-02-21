use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{CandleResolution, Side};

/// A trade from the WebSocket stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamTrade {
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub order_id: String,
}

/// WebSocket trade update message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketTradeUpdate {
    pub last_update_id: u64,
    pub update_id: u64,
    pub market_symbol: String,
    pub trades: Vec<StreamTrade>,
}

/// Orderbook entry from the delta stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderbookEntry {
    pub price: f64,
    pub size: f64,
}

/// WebSocket delta (orderbook) update message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketDeltaUpdate {
    pub e: String,
    pub last_update_id: u64,
    pub update_id: u64,
    pub market_symbol: String,
    pub asks: Vec<OrderbookEntry>,
    pub bids: Vec<OrderbookEntry>,
    pub timestamp: u64,
}

/// Fill information within an account update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountFill {
    pub side: Side,
    pub quantity: f64,
    pub remaining: f64,
    pub price: f64,
    pub order_id: String,
    pub market_id: u32,
    pub maker_id: u32,
    pub taker_id: u32,
    pub sender_tracking_id: Option<u64>,
}

/// Place information within an account update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPlace {
    pub side: Side,
    pub current_size: f64,
    pub price: f64,
    pub market_id: u32,
}

/// Cancel information within an account update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountCancel {
    pub side: Side,
    pub current_size: f64,
    pub price: f64,
    pub market_id: u32,
}

/// WebSocket account update message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketAccountUpdate {
    pub last_update_id: u64,
    pub update_id: u64,
    pub account_id: u32,
    pub fills: HashMap<String, AccountFill>,
    pub places: HashMap<String, AccountPlace>,
    pub cancels: HashMap<String, AccountCancel>,
    pub balances: HashMap<String, f64>,
}

/// WebSocket candle update message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketCandleUpdate {
    pub res: CandleResolution,
    pub mid: f64,
    pub t: u64,
    pub o: f64,
    pub h: f64,
    pub l: f64,
    pub c: f64,
    pub v: f64,
}

/// Enum wrapping all WebSocket message types.
#[derive(Debug, Clone)]
pub enum WebSocketMessage {
    Trade(WebSocketTradeUpdate),
    Delta(WebSocketDeltaUpdate),
    Account(WebSocketAccountUpdate),
    Candle(WebSocketCandleUpdate),
}
