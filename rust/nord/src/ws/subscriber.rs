use tokio::sync::broadcast;

use super::events::*;

/// Typed subscription for orderbook delta updates.
pub struct OrderbookSubscription {
    rx: broadcast::Receiver<WebSocketDeltaUpdate>,
}

impl OrderbookSubscription {
    /// Create a new orderbook subscription from a broadcast receiver.
    pub fn new(rx: broadcast::Receiver<WebSocketDeltaUpdate>) -> Self {
        Self { rx }
    }

    /// Receive the next update. Returns `None` if the channel is closed.
    pub async fn next(&mut self) -> Option<WebSocketDeltaUpdate> {
        loop {
            match self.rx.recv().await {
                Ok(msg) => return Some(msg),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("orderbook subscription lagged by {n} messages");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

/// Typed subscription for trade updates.
pub struct TradeSubscription {
    rx: broadcast::Receiver<WebSocketTradeUpdate>,
}

impl TradeSubscription {
    /// Create a new trade subscription from a broadcast receiver.
    pub fn new(rx: broadcast::Receiver<WebSocketTradeUpdate>) -> Self {
        Self { rx }
    }

    /// Receive the next update. Returns `None` if the channel is closed.
    pub async fn next(&mut self) -> Option<WebSocketTradeUpdate> {
        loop {
            match self.rx.recv().await {
                Ok(msg) => return Some(msg),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("trade subscription lagged by {n} messages");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

/// Typed subscription for account updates.
pub struct AccountSubscription {
    rx: broadcast::Receiver<WebSocketAccountUpdate>,
}

impl AccountSubscription {
    /// Create a new account subscription from a broadcast receiver.
    pub fn new(rx: broadcast::Receiver<WebSocketAccountUpdate>) -> Self {
        Self { rx }
    }

    /// Receive the next update. Returns `None` if the channel is closed.
    pub async fn next(&mut self) -> Option<WebSocketAccountUpdate> {
        loop {
            match self.rx.recv().await {
                Ok(msg) => return Some(msg),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("account subscription lagged by {n} messages");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

/// Typed subscription for candle updates.
pub struct CandleSubscription {
    rx: broadcast::Receiver<WebSocketCandleUpdate>,
}

impl CandleSubscription {
    /// Create a new candle subscription from a broadcast receiver.
    pub fn new(rx: broadcast::Receiver<WebSocketCandleUpdate>) -> Self {
        Self { rx }
    }

    /// Receive the next update. Returns `None` if the channel is closed.
    pub async fn next(&mut self) -> Option<WebSocketCandleUpdate> {
        loop {
            match self.rx.recv().await {
                Ok(msg) => return Some(msg),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("candle subscription lagged by {n} messages");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}
