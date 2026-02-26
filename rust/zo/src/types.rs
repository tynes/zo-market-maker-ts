use rust_decimal::Decimal;
use serde::Deserialize;

/// Binance combined stream envelope: `{"stream":"btcusdt@bookTicker","data":{...}}`
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CombinedStreamMsg {
    pub stream: String,
    pub data: BookTickerMsg,
}

/// Binance bookTicker payload.
///
/// Field names match the Binance API:
///   e  = event type
///   E  = event time (ms)
///   T  = transaction time (ms)
///   s  = symbol
///   b  = best bid price (string)
///   B  = best bid qty (string)
///   a  = best ask price (string)
///   A  = best ask qty (string)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct BookTickerMsg {
    #[serde(default)]
    pub e: String,
    #[serde(default)]
    #[serde(rename = "E")]
    pub event_time: u64,
    #[serde(default)]
    #[serde(rename = "T")]
    pub transaction_time: u64,
    pub s: String,
    pub b: String,
    #[serde(rename = "B")]
    pub bid_qty: String,
    pub a: String,
    #[serde(rename = "A")]
    pub ask_qty: String,
}

/// A quote for order placement with side, price, and size.
#[derive(Debug, Clone)]
pub struct Quote {
    pub side: nord::Side,
    pub price: Decimal,
    pub size: Decimal,
}
