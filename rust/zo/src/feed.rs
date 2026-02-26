//! Binance Futures WebSocket price feed.
//!
//! Provides two interfaces:
//! - [`run_feed`]: CLI mode — streams prices to stdout (used by `zo feed`).
//! - [`BinancePriceFeed`]: Struct mode — publishes [`MidPrice`] via a `watch`
//!   channel for consumption by the market maker and monitor.

use std::io::{self, BufWriter, Write};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::watch;
use tokio::time::{self, Instant};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::error::ZoError;
use crate::output;
use crate::types::BookTickerMsg;

const PING_INTERVAL: Duration = Duration::from_secs(30);
const PONG_TIMEOUT: Duration = Duration::from_secs(10);
const STALE_THRESHOLD: Duration = Duration::from_secs(60);
const STALE_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);
const BINANCE_FUTURES_WS: &str = "wss://fstream.binance.com/ws";

// ---------------------------------------------------------------------------
// BinancePriceFeed (struct mode for bot / monitor)
// ---------------------------------------------------------------------------

/// Live Binance Futures mid-price, published via `watch` channel.
///
/// Spawns a background WebSocket task with auto-reconnect, heartbeat, and
/// stale connection detection. Consumers call [`subscribe_price`] to get a
/// `watch::Receiver` that is updated on every book ticker message.
pub struct BinancePriceFeed {
    price_tx: watch::Sender<Option<nord::MidPrice>>,
    price_rx: watch::Receiver<Option<nord::MidPrice>>,
    cancel: CancellationToken,
    ws_url: String,
}

impl BinancePriceFeed {
    /// Create a new feed for the given lowercase symbol (e.g. `"btcusdt"`).
    ///
    /// Does **not** connect yet — call [`connect`] to start.
    pub fn new(symbol: &str) -> Self {
        let (price_tx, price_rx) = watch::channel(None);
        let ws_url = format!("{BINANCE_FUTURES_WS}/{symbol}@bookTicker");
        Self {
            price_tx,
            price_rx,
            cancel: CancellationToken::new(),
            ws_url,
        }
    }

    /// Start the background WebSocket connection.
    pub fn connect(&self) {
        let url = self.ws_url.clone();
        let tx = self.price_tx.clone();
        let cancel = self.cancel.clone();

        tokio::spawn(async move {
            info!(url = %url, "binance feed starting");
            loop {
                match run_price_connection(&url, &tx, &cancel).await {
                    Ok(()) => {
                        info!("binance feed stopped gracefully");
                        return;
                    }
                    Err(e) => {
                        error!(error = %e, "binance connection error");
                        if cancel.is_cancelled() {
                            return;
                        }
                        info!(delay = ?RECONNECT_DELAY, "reconnecting binance");
                        tokio::select! {
                            _ = time::sleep(RECONNECT_DELAY) => {}
                            _ = cancel.cancelled() => return,
                        }
                    }
                }
            }
        });
    }

    /// Latest mid-price snapshot (lock-free read).
    pub fn get_mid_price(&self) -> Option<nord::MidPrice> {
        *self.price_rx.borrow()
    }

    /// Subscribe to price updates.
    pub fn subscribe_price(&self) -> watch::Receiver<Option<nord::MidPrice>> {
        self.price_rx.clone()
    }

    /// Gracefully shut down the background task.
    pub fn close(&self) {
        self.cancel.cancel();
    }
}

/// Single WebSocket connection that parses book tickers into [`MidPrice`] and
/// sends them via the `watch` channel.
async fn run_price_connection(
    url: &str,
    tx: &watch::Sender<Option<nord::MidPrice>>,
    cancel: &CancellationToken,
) -> Result<(), ZoError> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
    let (mut sink, mut stream) = ws_stream.split();
    info!("binance connected");

    let mut last_message_time = Instant::now();
    let mut ping_interval = time::interval(PING_INTERVAL);
    ping_interval.tick().await;
    let mut stale_interval = time::interval(STALE_CHECK_INTERVAL);
    stale_interval.tick().await;
    let mut pong_deadline: Option<Instant> = None;

    loop {
        let pong_timeout_fut = match pong_deadline {
            Some(d) => tokio::time::sleep_until(d),
            None => tokio::time::sleep_until(Instant::now() + Duration::from_secs(86400)),
        };
        let pong_active = pong_deadline.is_some();

        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_message_time = Instant::now();
                        if let Some(mid) = parse_book_ticker(&text) {
                            let _ = tx.send(Some(mid));
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        last_message_time = Instant::now();
                        sink.send(Message::Pong(data)).await?;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_message_time = Instant::now();
                        pong_deadline = None;
                    }
                    Some(Ok(Message::Close(_))) => return Err(ZoError::ConnectionClosed),
                    Some(Ok(_)) => { last_message_time = Instant::now(); }
                    Some(Err(e)) => return Err(ZoError::WebSocket(Box::new(e))),
                    None => return Err(ZoError::ConnectionClosed),
                }
            }
            _ = ping_interval.tick() => {
                sink.send(Message::Ping(vec![])).await?;
                pong_deadline = Some(Instant::now() + PONG_TIMEOUT);
            }
            _ = stale_interval.tick() => {
                let elapsed = last_message_time.elapsed();
                if elapsed > STALE_THRESHOLD {
                    return Err(ZoError::StaleConnection(elapsed.as_millis() as u64));
                }
            }
            _ = pong_timeout_fut, if pong_active => {
                return Err(ZoError::PongTimeout);
            }
            _ = cancel.cancelled() => {
                let _ = sink.send(Message::Close(None)).await;
                return Ok(());
            }
        }
    }
}

/// Parse a Binance bookTicker JSON into a [`MidPrice`].
///
/// Returns `None` on parse failure (logged at debug level).
fn parse_book_ticker(text: &str) -> Option<nord::MidPrice> {
    let msg: BookTickerMsg = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            debug!(error = %e, "failed to parse book ticker");
            return None;
        }
    };
    let bid: f64 = msg.b.parse().ok()?;
    let ask: f64 = msg.a.parse().ok()?;
    let mid = (bid + ask) * 0.5;
    let timestamp = epoch_ms();
    Some(nord::MidPrice {
        mid,
        bid,
        ask,
        timestamp,
    })
}

/// Current wall-clock time in epoch milliseconds.
fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// run_feed (CLI stdout mode, used by `zo feed`)
// ---------------------------------------------------------------------------

/// Build the combined stream URL for the given symbols.
fn build_url(symbols: &[String]) -> String {
    let streams: Vec<String> = symbols
        .iter()
        .map(|s| format!("{}@bookTicker", s.to_lowercase()))
        .collect();
    format!(
        "wss://fstream.binance.com/stream?streams={}",
        streams.join("/")
    )
}

/// Outer reconnection loop. Runs until cancelled. Streams to stdout.
pub async fn run_feed(symbols: &[String], json_mode: bool, cancel: CancellationToken) {
    let url = build_url(symbols);
    info!(url = %url, "starting feed");

    loop {
        match run_single_connection(&url, json_mode, &cancel).await {
            Ok(()) => {
                info!("feed stopped gracefully");
                return;
            }
            Err(e) => {
                error!(error = %e, "connection error");
                if cancel.is_cancelled() {
                    return;
                }
                info!(delay = ?RECONNECT_DELAY, "reconnecting");
                tokio::select! {
                    _ = time::sleep(RECONNECT_DELAY) => {}
                    _ = cancel.cancelled() => {
                        info!("shutdown during reconnect wait");
                        return;
                    }
                }
            }
        }
    }
}

/// Single WebSocket connection lifetime (stdout mode).
async fn run_single_connection(
    url: &str,
    json_mode: bool,
    cancel: &CancellationToken,
) -> Result<(), ZoError> {
    info!("connecting");

    let (ws_stream, _response) = tokio_tungstenite::connect_async(url).await?;
    let (mut sink, mut stream) = ws_stream.split();

    info!("connected");

    let stdout = io::stdout().lock();
    let mut writer = BufWriter::new(stdout);
    let mut buf = String::with_capacity(512);

    let mut last_message_time = Instant::now();
    let mut ping_interval = time::interval(PING_INTERVAL);
    ping_interval.tick().await;

    let mut stale_interval = time::interval(STALE_CHECK_INTERVAL);
    stale_interval.tick().await;

    let mut pong_deadline: Option<Instant> = None;

    loop {
        let pong_timeout_fut = match pong_deadline {
            Some(deadline) => tokio::time::sleep_until(deadline),
            None => tokio::time::sleep_until(Instant::now() + Duration::from_secs(86400)),
        };
        let pong_active = pong_deadline.is_some();

        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_message_time = Instant::now();
                        if let Err(e) = output::handle_message(&text, json_mode, &mut buf, &mut writer) {
                            debug!(error = %e, "failed to handle message");
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        last_message_time = Instant::now();
                        sink.send(Message::Pong(data)).await?;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_message_time = Instant::now();
                        pong_deadline = None;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(ZoError::ConnectionClosed);
                    }
                    Some(Ok(_)) => {
                        last_message_time = Instant::now();
                    }
                    Some(Err(e)) => {
                        return Err(ZoError::WebSocket(Box::new(e)));
                    }
                    None => {
                        return Err(ZoError::ConnectionClosed);
                    }
                }
            }
            _ = ping_interval.tick() => {
                sink.send(Message::Ping(vec![])).await?;
                pong_deadline = Some(Instant::now() + PONG_TIMEOUT);
            }
            _ = stale_interval.tick() => {
                let elapsed = last_message_time.elapsed();
                if elapsed > STALE_THRESHOLD {
                    let ms = elapsed.as_millis() as u64;
                    warn!(elapsed_ms = ms, "connection stale");
                    return Err(ZoError::StaleConnection(ms));
                }
            }
            _ = pong_timeout_fut, if pong_active => {
                warn!("pong timeout");
                return Err(ZoError::PongTimeout);
            }
            _ = cancel.cancelled() => {
                let _ = sink.send(Message::Close(None)).await;
                let _ = writer.flush();
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_book_ticker_to_mid_price() {
        let json = r#"{"s":"BTCUSDT","b":"50000.00","a":"50010.00","B":"1.5","A":"2.0"}"#;
        let mid = parse_book_ticker(json).unwrap();
        assert!((mid.bid - 50000.0).abs() < 1e-6);
        assert!((mid.ask - 50010.0).abs() < 1e-6);
        assert!((mid.mid - 50005.0).abs() < 1e-6);
        assert!(mid.timestamp > 0);
    }

    #[test]
    fn test_mid_price_calculation() {
        let json = r#"{"s":"ETHUSDT","b":"3000.50","a":"3001.50","B":"10","A":"10"}"#;
        let mid = parse_book_ticker(json).unwrap();
        // (3000.50 + 3001.50) / 2 = 3001.0
        assert!((mid.mid - 3001.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_invalid_json_returns_none() {
        assert!(parse_book_ticker("not json").is_none());
    }

    #[test]
    fn test_parse_missing_fields_returns_none() {
        let json = r#"{"s":"BTCUSDT","b":"invalid","a":"50010.00"}"#;
        assert!(parse_book_ticker(json).is_none());
    }
}
