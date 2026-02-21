use thiserror::Error;

#[derive(Error, Debug)]
pub enum NordError {
    #[error("HTTP error {status}: {message}")]
    Http { status: u16, message: String },

    #[error("protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("protobuf encode error: {0}")]
    ProtobufEncode(#[from] prost::EncodeError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("signing error: {0}")]
    Signing(String),

    #[error("session invalid: {0}")]
    SessionInvalid(String),

    #[error("no account found")]
    NoAccount,

    #[error("market not found: {0}")]
    MarketNotFound(u32),

    #[error("token not found: {0}")]
    TokenNotFound(u32),

    #[error("receipt error: {0}")]
    ReceiptError(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[cfg(feature = "solana")]
    #[error("solana error: {0}")]
    Solana(String),
}

pub type Result<T> = std::result::Result<T, NordError>;
