use std::io::{self, BufWriter, Write};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::time::{self, Instant};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::error::FeedError;
use crate::output;

const PING_INTERVAL: Duration = Duration::from_secs(30);
const PONG_TIMEOUT: Duration = Duration::from_secs(10);
const STALE_THRESHOLD: Duration = Duration::from_secs(60);
const STALE_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

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

/// Outer reconnection loop. Runs until cancelled.
pub async fn run_feed(
    symbols: &[String],
    json_mode: bool,
    cancel: CancellationToken,
) {
    let url = build_url(symbols);
    info!(url = %url, "starting feed");

    loop {
        match run_single_connection(&url, json_mode, &cancel).await {
            Ok(()) => {
                // Graceful shutdown
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

/// Single WebSocket connection lifetime.
async fn run_single_connection(
    url: &str,
    json_mode: bool,
    cancel: &CancellationToken,
) -> Result<(), FeedError> {
    info!("connecting");

    let (ws_stream, _response) = tokio_tungstenite::connect_async(url).await?;
    let (mut sink, mut stream) = ws_stream.split();

    info!("connected");

    let stdout = io::stdout().lock();
    let mut writer = BufWriter::new(stdout);
    let mut buf = String::with_capacity(512);

    let mut last_message_time = Instant::now();
    let mut ping_interval = time::interval(PING_INTERVAL);
    ping_interval.tick().await; // consume the immediate first tick

    let mut stale_interval = time::interval(STALE_CHECK_INTERVAL);
    stale_interval.tick().await; // consume the immediate first tick

    let mut pong_deadline: Option<Instant> = None;

    loop {
        // Build the pong timeout future: either sleep_until(deadline) or pending forever.
        let pong_timeout_fut = match pong_deadline {
            Some(deadline) => tokio::time::sleep_until(deadline),
            None => tokio::time::sleep_until(Instant::now() + Duration::from_secs(86400)),
            // ^-- effectively pending; we'll never reach 24h without something else firing
        };
        let pong_active = pong_deadline.is_some();

        tokio::select! {
            // Branch 1: WebSocket message
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_message_time = Instant::now();
                        if let Err(e) = output::handle_message(&text, json_mode, &mut buf, &mut writer) {
                            debug!(error = %e, "failed to handle message");
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        debug!("received server ping");
                        last_message_time = Instant::now();
                        sink.send(Message::Pong(data)).await?;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        debug!("received pong");
                        last_message_time = Instant::now();
                        pong_deadline = None;
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("received close frame");
                        return Err(FeedError::ConnectionClosed);
                    }
                    Some(Ok(_)) => {
                        // Binary or Frame — ignore
                        last_message_time = Instant::now();
                    }
                    Some(Err(e)) => {
                        return Err(FeedError::WebSocket(e));
                    }
                    None => {
                        return Err(FeedError::ConnectionClosed);
                    }
                }
            }

            // Branch 2: Ping interval
            _ = ping_interval.tick() => {
                debug!("sending ping");
                sink.send(Message::Ping(vec![].into())).await?;
                pong_deadline = Some(Instant::now() + PONG_TIMEOUT);
            }

            // Branch 3: Stale check
            _ = stale_interval.tick() => {
                let elapsed = last_message_time.elapsed();
                if elapsed > STALE_THRESHOLD {
                    let ms = elapsed.as_millis() as u64;
                    warn!(elapsed_ms = ms, "connection stale");
                    return Err(FeedError::StaleConnection(ms));
                }
            }

            // Branch 4: Pong timeout
            _ = pong_timeout_fut, if pong_active => {
                warn!("pong timeout");
                return Err(FeedError::PongTimeout);
            }

            // Branch 5: Shutdown
            _ = cancel.cancelled() => {
                info!("shutdown requested, sending close frame");
                let _ = sink.send(Message::Close(None)).await;
                // Flush any remaining output
                let _ = writer.flush();
                return Ok(());
            }
        }
    }
}
