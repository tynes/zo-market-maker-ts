pub mod events;
pub mod subscriber;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::error::NordError;

use events::*;

/// WebSocket client for the Nord exchange.
///
/// Manages a persistent connection with auto-reconnect and heartbeat.
/// Dispatches typed messages via broadcast channels.
#[derive(Debug)]
pub struct NordWebSocketClient {
    url: String,
    trade_tx: broadcast::Sender<WebSocketTradeUpdate>,
    delta_tx: broadcast::Sender<WebSocketDeltaUpdate>,
    account_tx: broadcast::Sender<WebSocketAccountUpdate>,
    candle_tx: broadcast::Sender<WebSocketCandleUpdate>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl NordWebSocketClient {
    /// Create a new WebSocket client (does not connect yet).
    pub fn new(url: String) -> Self {
        let (trade_tx, _) = broadcast::channel(256);
        let (delta_tx, _) = broadcast::channel(256);
        let (account_tx, _) = broadcast::channel(256);
        let (candle_tx, _) = broadcast::channel(256);

        Self {
            url,
            trade_tx,
            delta_tx,
            account_tx,
            candle_tx,
            shutdown_tx: None,
        }
    }

    /// Subscribe to trade updates.
    pub fn subscribe_trades(&self) -> broadcast::Receiver<WebSocketTradeUpdate> {
        self.trade_tx.subscribe()
    }

    /// Subscribe to delta (orderbook) updates.
    pub fn subscribe_deltas(&self) -> broadcast::Receiver<WebSocketDeltaUpdate> {
        self.delta_tx.subscribe()
    }

    /// Subscribe to account updates.
    pub fn subscribe_accounts(&self) -> broadcast::Receiver<WebSocketAccountUpdate> {
        self.account_tx.subscribe()
    }

    /// Subscribe to candle updates.
    pub fn subscribe_candles(&self) -> broadcast::Receiver<WebSocketCandleUpdate> {
        self.candle_tx.subscribe()
    }

    /// Connect and start processing messages in the background.
    pub fn connect(&mut self) {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let url = self.url.clone();
        let trade_tx = self.trade_tx.clone();
        let delta_tx = self.delta_tx.clone();
        let account_tx = self.account_tx.clone();
        let candle_tx = self.candle_tx.clone();

        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_rx;
            loop {
                match Self::run_connection(
                    &url,
                    &trade_tx,
                    &delta_tx,
                    &account_tx,
                    &candle_tx,
                    &mut shutdown_rx,
                )
                .await
                {
                    Ok(()) => {
                        info!("WebSocket connection closed gracefully");
                        break;
                    }
                    Err(e) => {
                        warn!("WebSocket connection error: {e}, reconnecting in 3s...");
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            }
        });
    }

    async fn run_connection(
        url: &str,
        trade_tx: &broadcast::Sender<WebSocketTradeUpdate>,
        delta_tx: &broadcast::Sender<WebSocketDeltaUpdate>,
        account_tx: &broadcast::Sender<WebSocketAccountUpdate>,
        candle_tx: &broadcast::Sender<WebSocketCandleUpdate>,
        shutdown_rx: &mut tokio::sync::oneshot::Receiver<()>,
    ) -> std::result::Result<(), NordError> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| NordError::WebSocket(format!("connect failed: {e}")))?;

        info!("WebSocket connected to {url}");

        let (mut write, mut read) = ws_stream.split();

        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
        let mut pong_timeout: Option<tokio::time::Instant> = None;

        loop {
            tokio::select! {
                _ = &mut *shutdown_rx => {
                    debug!("WebSocket shutdown requested");
                    let _ = write.close().await;
                    return Ok(());
                }
                _ = ping_interval.tick() => {
                    if let Some(deadline) = pong_timeout {
                        if tokio::time::Instant::now() > deadline {
                            return Err(NordError::WebSocket("pong timeout".into()));
                        }
                    }
                    let _ = write.send(Message::Ping(vec![])).await;
                    pong_timeout = Some(tokio::time::Instant::now() + Duration::from_secs(10));
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            Self::dispatch_message(
                                &text,
                                trade_tx,
                                delta_tx,
                                account_tx,
                                candle_tx,
                            );
                        }
                        Some(Ok(Message::Pong(_))) => {
                            pong_timeout = None;
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = write.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) => {
                            return Err(NordError::WebSocket("server closed connection".into()));
                        }
                        Some(Err(e)) => {
                            return Err(NordError::WebSocket(format!("read error: {e}")));
                        }
                        None => {
                            return Err(NordError::WebSocket("stream ended".into()));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn dispatch_message(
        text: &str,
        trade_tx: &broadcast::Sender<WebSocketTradeUpdate>,
        delta_tx: &broadcast::Sender<WebSocketDeltaUpdate>,
        account_tx: &broadcast::Sender<WebSocketAccountUpdate>,
        candle_tx: &broadcast::Sender<WebSocketCandleUpdate>,
    ) {
        // Try to determine message type from JSON structure.
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            // Wrapped messages: { "trades": ... }, { "delta": ... }, { "account": ... }
            if let Some(trades) = value.get("trades") {
                if let Ok(update) =
                    serde_json::from_value::<WebSocketTradeUpdate>(trades.clone())
                {
                    let _ = trade_tx.send(update);
                    return;
                }
            }
            if let Some(delta) = value.get("delta") {
                if let Ok(update) =
                    serde_json::from_value::<WebSocketDeltaUpdate>(delta.clone())
                {
                    let _ = delta_tx.send(update);
                    return;
                }
            }
            if let Some(account) = value.get("account") {
                if let Ok(update) =
                    serde_json::from_value::<WebSocketAccountUpdate>(account.clone())
                {
                    let _ = account_tx.send(update);
                    return;
                }
            }
            // Candle updates are sent as bare objects with "res" field.
            if value.get("res").is_some() {
                if let Ok(update) =
                    serde_json::from_value::<WebSocketCandleUpdate>(value)
                {
                    let _ = candle_tx.send(update);
                    return;
                }
            }

            debug!("unrecognized WebSocket message: {text}");
        } else {
            error!("failed to parse WebSocket message as JSON: {text}");
        }
    }

    /// Close the WebSocket connection.
    pub fn close(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}
