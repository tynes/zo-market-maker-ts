use thiserror::Error;

/// Unified error type for the zo binary crate.
#[derive(Debug, Error)]
pub enum ZoError {
    /// WebSocket transport error (Binance feed, etc.).
    #[error("websocket error: {0}")]
    WebSocket(Box<tokio_tungstenite::tungstenite::Error>),

    /// JSON serialization/deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Float parsing error.
    #[error("float parse error: {0}")]
    FloatParse(#[from] std::num::ParseFloatError),

    /// WebSocket connection closed unexpectedly.
    #[error("connection closed")]
    ConnectionClosed,

    /// Pong not received within timeout.
    #[error("pong timeout")]
    PongTimeout,

    /// No messages received within the stale threshold.
    #[error("stale connection: {0}ms since last message")]
    StaleConnection(u64),

    /// Error propagated from the nord SDK.
    #[error("nord error: {0}")]
    Nord(#[from] nord::NordError),

    /// Market symbol not found on the exchange.
    #[error("market not found: {0}")]
    MarketNotFound(String),

    /// Wallet has no exchange account.
    #[error("no account found — deposit funds on 01.xyz to create one")]
    NoAccount,

    /// Configuration or environment error.
    #[error("config error: {0}")]
    Config(String),
}

impl From<tokio_tungstenite::tungstenite::Error> for ZoError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        ZoError::WebSocket(Box::new(e))
    }
}

impl From<std::io::Error> for ZoError {
    fn from(e: std::io::Error) -> Self {
        // Map IO errors (stdout broken pipe, etc.) to ConnectionClosed.
        tracing::debug!("IO error: {e}");
        ZoError::ConnectionClosed
    }
}
