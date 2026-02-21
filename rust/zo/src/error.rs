use thiserror::Error;

/// Errors that can occur during price feed operations.
#[derive(Debug, Error)]
pub enum FeedError {
    #[error("websocket error: {0}")]
    WebSocket(Box<tokio_tungstenite::tungstenite::Error>),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("float parse error: {0}")]
    FloatParse(#[from] std::num::ParseFloatError),

    #[error("connection closed")]
    ConnectionClosed,

    #[error("pong timeout")]
    PongTimeout,

    #[error("stale connection: {0}ms since last message")]
    StaleConnection(u64),
}

impl From<tokio_tungstenite::tungstenite::Error> for FeedError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        FeedError::WebSocket(Box::new(e))
    }
}
