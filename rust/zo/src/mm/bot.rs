//! Market maker bot orchestrator.
//!
//! Ports `src/bots/mm/index.ts`. Uses a `tokio::select!` event loop instead
//! of callbacks + lodash throttle.

use std::sync::Arc;
use std::time::Duration;

use nord::{NordUser, Side};
use rust_decimal::Decimal;
use tokio::time::{self, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::client::{create_zo_client, ZoClient};
use crate::error::ZoError;
use crate::fair_price::{FairPriceCalculator, FairPriceConfig};
use crate::feed::BinancePriceFeed;
use crate::mm::config::MarketMakerConfig;
use crate::mm::position::{PositionConfig, PositionTracker};
use crate::mm::quoter::Quoter;
use crate::orders::{cancel_orders, update_quotes, CachedOrder};

/// Top-level market maker.
pub struct MarketMaker {
    config: MarketMakerConfig,
    private_key: String,
}

// ---------------------------------------------------------------------------
// Helpers (pure, testable)
// ---------------------------------------------------------------------------

/// Derive Binance Futures symbol from an exchange market symbol.
///
/// `"BTC-PERP"` → `"btcusdt"`, `"ETH-PERP"` → `"ethusdt"`.
pub fn derive_binance_symbol(market_symbol: &str) -> String {
    let base = market_symbol
        .split('-')
        .next()
        .unwrap_or(market_symbol)
        .to_lowercase()
        .replace("usd", "");
    format!("{base}usdt")
}

/// Convert server API orders to [`CachedOrder`]s.
pub fn map_api_orders_to_cached(orders: &[nord::OpenOrder]) -> Vec<CachedOrder> {
    orders
        .iter()
        .map(|o| CachedOrder {
            order_id: o.order_id,
            side: o.side,
            price: Decimal::from_f64_retain(o.price).unwrap_or_default(),
            size: Decimal::from_f64_retain(o.size).unwrap_or_default(),
        })
        .collect()
}

impl MarketMaker {
    /// Create a new market maker (does not connect yet).
    pub fn new(config: MarketMakerConfig, private_key: String) -> Self {
        Self {
            config,
            private_key,
        }
    }

    /// Run the market maker until `cancel` is triggered.
    ///
    /// This is the main entry point. It:
    /// 1. Connects to the exchange and Binance.
    /// 2. Finds the market and initialises components.
    /// 3. Warms up the fair price calculator.
    /// 4. Enters the main event loop (quote, fill, sync, status).
    /// 5. On shutdown, cancels all active orders.
    pub async fn run(&self, cancel: CancellationToken) -> Result<(), ZoError> {
        info!("starting market maker");

        // --- Initialise exchange client ---
        let client = create_zo_client(&self.private_key).await?;
        let ZoClient {
            nord,
            user,
            account_id,
        } = client;

        // --- Find market ---
        let market = nord
            .markets
            .iter()
            .find(|m| {
                m.symbol
                    .to_uppercase()
                    .starts_with(&self.config.symbol.to_uppercase())
            })
            .ok_or_else(|| {
                let available: Vec<_> = nord.markets.iter().map(|m| m.symbol.as_str()).collect();
                ZoError::MarketNotFound(format!(
                    "\"{}\" not found. Available: {}",
                    self.config.symbol,
                    available.join(", ")
                ))
            })?;

        let market_id = market.market_id;
        let market_symbol = market.symbol.clone();
        let binance_symbol = derive_binance_symbol(&market_symbol);

        info!(
            market = %market_symbol,
            binance = %binance_symbol,
            spread_bps = self.config.spread_bps,
            order_size_usd = self.config.order_size_usd,
            close_threshold_usd = self.config.close_threshold_usd,
            "CONFIG"
        );

        // --- Build strategy components ---
        let fair_price_calc = FairPriceCalculator::new(FairPriceConfig {
            window_ms: self.config.fair_price_window_ms,
            min_samples: self.config.warmup_seconds,
        });

        let position_tracker = PositionTracker::new(PositionConfig {
            close_threshold_usd: self.config.close_threshold_usd,
            sync_interval_ms: self.config.position_sync_interval_ms,
        });

        let quoter = Quoter::new(
            market.price_decimals,
            market.size_decimals,
            self.config.spread_bps,
            self.config.take_profit_bps,
            self.config.order_size_usd,
        );

        // --- Build streams ---
        let ws = nord.create_websocket_client(
            &[],                                  // no trades
            std::slice::from_ref(&market_symbol), // orderbook deltas
            &[account_id],                        // account events
            &[],                                  // no candles
        );

        let mut orderbook = nord::OrderbookStream::new(
            market_symbol.clone(),
            (*nord).clone(),
            ws.subscribe_deltas(),
        );
        orderbook.connect().await?;

        let mut account_stream =
            nord::AccountStream::new(account_id, ws.subscribe_accounts(), Arc::clone(&nord));

        let binance_feed = BinancePriceFeed::new(&binance_symbol);

        // --- Start connections ---
        // The WebSocket client needs to be started for broadcasts to flow.
        // We need to move `ws` since `connect` takes `&mut self`.
        let mut ws = ws;
        ws.connect();

        account_stream.connect();
        binance_feed.connect();

        // --- Sync initial state ---
        let mut active_orders = {
            let mut u = NordUser::from_private_key(Arc::clone(&nord), &self.private_key)?;
            u.refresh_session().await?;
            u.update_account_id().await?;
            u.fetch_info().await?;
            let api_orders: Vec<_> = u
                .orders
                .get(&account_id.to_string())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|o| o.market_id == market_id)
                .collect();
            let cached = map_api_orders_to_cached(&api_orders);
            if !cached.is_empty() {
                info!(count = cached.len(), "synced existing orders");
            }
            account_stream.sync_initial_orders(&api_orders);
            cached
        };

        // Start position sync.
        position_tracker.start_sync(Arc::clone(&nord), account_id, market_id, cancel.clone());

        // --- Prepare event loop state ---
        let mut binance_rx = binance_feed.subscribe_price();
        let mut zo_price_rx = orderbook.subscribe_price();
        let mut fill_rx = account_stream
            .take_fill_rx()
            .expect("fill_rx already taken");

        let mut fair_price_calc = fair_price_calc;
        let mut last_logged_sample_count: isize = -1;
        let mut last_update_time = Instant::now();

        let mut order_sync_interval =
            time::interval(Duration::from_millis(self.config.order_sync_interval_ms));
        order_sync_interval.tick().await;

        let mut status_interval =
            time::interval(Duration::from_millis(self.config.status_interval_ms));
        status_interval.tick().await;

        let update_throttle = Duration::from_millis(self.config.update_throttle_ms);

        info!("warming up price feeds...");

        // --- Main event loop ---
        loop {
            tokio::select! {
                // Binance price update
                result = binance_rx.changed() => {
                    if result.is_err() { continue; }
                    let now_ms = epoch_ms();
                    let binance_mid = match *binance_rx.borrow_and_update() {
                        Some(ref p) => *p,
                        None => continue,
                    };

                    // Sample fair price if both feeds are fresh.
                    if let Some(zo_mid) = orderbook.get_mid_price() {
                        if (binance_mid.timestamp as i64 - zo_mid.timestamp as i64).unsigned_abs() < 1000 {
                            fair_price_calc.add_sample(zo_mid.mid, binance_mid.mid, now_ms);
                        }
                    }

                    let fair = match fair_price_calc.get_fair_price(binance_mid.mid, now_ms) {
                        Some(f) => f,
                        None => {
                            log_warmup(&fair_price_calc, &binance_mid, orderbook.get_mid_price(), &mut last_logged_sample_count, self.config.warmup_seconds, now_ms);
                            continue;
                        }
                    };

                    // Log "ready" on first valid fair price.
                    if last_logged_sample_count < self.config.warmup_seconds as isize {
                        last_logged_sample_count = self.config.warmup_seconds as isize;
                        info!(fair_price = format!("{fair:.2}"), "ready");
                    }

                    // Throttled update.
                    if last_update_time.elapsed() >= update_throttle {
                        last_update_time = Instant::now();
                        execute_update(
                            fair, &user, market_id, &position_tracker,
                            &quoter, &orderbook, &mut active_orders,
                            &self.config,
                        ).await;
                    }
                }

                // Zo orderbook price update — just sample the fair price.
                result = zo_price_rx.changed() => {
                    if result.is_err() { continue; }
                    let now_ms = epoch_ms();
                    if let Some(ref zo_mid) = *zo_price_rx.borrow_and_update() {
                        if let Some(binance_mid) = binance_feed.get_mid_price() {
                            if (zo_mid.timestamp as i64 - binance_mid.timestamp as i64).unsigned_abs() < 1000 {
                                fair_price_calc.add_sample(zo_mid.mid, binance_mid.mid, now_ms);
                            }
                        }
                    }
                }

                // Fill event → update position, maybe enter close mode.
                Some(fill) = fill_rx.recv() => {
                    let dir = if fill.side == Side::Bid { "buy" } else { "sell" };
                    info!(
                        side = dir,
                        price = format!("{:.2}", fill.price),
                        size = fill.size,
                        "FILL"
                    );
                    position_tracker.apply_fill(fill.side, fill.size);

                    // If entering close mode, cancel all immediately.
                    if position_tracker.is_close_mode(fill.price)
                        && !active_orders.is_empty()
                    {
                        if let Err(e) = cancel_orders(&user, &active_orders).await {
                            error!(error = %e, "failed to cancel on close mode");
                        }
                        active_orders.clear();
                    }
                }

                // Periodic order sync from server.
                _ = order_sync_interval.tick() => {
                    match sync_orders_from_server(&user, account_id, market_id).await {
                        Ok(orders) => { active_orders = orders; }
                        Err(e) => { error!(error = %e, "order sync error"); }
                    }
                }

                // Periodic status log.
                _ = status_interval.tick() => {
                    log_status(&position_tracker, &active_orders);
                }

                // Shutdown.
                _ = cancel.cancelled() => {
                    info!("shutting down");
                    break;
                }
            }
        }

        // --- Shutdown: cancel all active orders ---
        if !active_orders.is_empty() {
            match cancel_orders(&user, &active_orders).await {
                Ok(()) => info!(count = active_orders.len(), "cancelled orders — goodbye"),
                Err(e) => error!(error = %e, "shutdown cancel error"),
            }
        } else {
            info!("no active orders — goodbye");
        }

        binance_feed.close();
        orderbook.close();
        account_stream.close();

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn execute_update(
    fair_price: f64,
    user: &NordUser,
    market_id: u32,
    position_tracker: &PositionTracker,
    quoter: &Quoter,
    orderbook: &nord::OrderbookStream,
    active_orders: &mut Vec<CachedOrder>,
    config: &MarketMakerConfig,
) {
    let ctx = position_tracker.get_quoting_context(fair_price);
    let pos = &ctx.position_state;

    if pos.size_base != 0.0 {
        let dir = if pos.is_long { "LONG" } else { "SHORT" };
        let mode = if pos.is_close_mode { " [CLOSE]" } else { "" };
        info!(
            direction = dir,
            size_base = format!("{:.6}", pos.size_base.abs()),
            size_usd = format!("{:.2}", pos.size_usd.abs()),
            mode,
            "POS"
        );
    }

    let bbo = orderbook.get_bbo();
    let quotes = quoter.get_quotes(&ctx, bbo.as_ref());

    if quotes.is_empty() {
        warn!("no quotes generated (order size too small)");
        return;
    }

    // Log the quotes.
    let bid = quotes.iter().find(|q| q.side == Side::Bid);
    let ask = quotes.iter().find(|q| q.side == Side::Ask);
    let spread_bps = if pos.is_close_mode {
        config.take_profit_bps
    } else {
        config.spread_bps
    };
    let mode = if pos.is_close_mode { "close" } else { "normal" };
    info!(
        bid = bid
            .map(|q| format!("${}", q.price))
            .unwrap_or_else(|| "--".into()),
        ask = ask
            .map(|q| format!("${}", q.price))
            .unwrap_or_else(|| "--".into()),
        fair = format!("${fair_price:.2}"),
        spread = format!("{spread_bps}bps"),
        mode,
        "QUOTE"
    );

    match update_quotes(user, market_id, active_orders, &quotes).await {
        Ok(new_orders) => *active_orders = new_orders,
        Err(e) => {
            error!(error = %e, "update error");
            active_orders.clear();
        }
    }
}

async fn sync_orders_from_server(
    user: &NordUser,
    account_id: u32,
    market_id: u32,
) -> Result<Vec<CachedOrder>, ZoError> {
    // Re-fetch user info. Since NordUser requires &mut for fetch_info, and
    // we only have &, we use the REST client directly.
    let account = user.nord.get_account(account_id).await?;
    let market_orders: Vec<_> = account
        .orders
        .iter()
        .filter(|o| o.market_id == market_id)
        .cloned()
        .collect();
    Ok(map_api_orders_to_cached(&market_orders))
}

fn log_warmup(
    calc: &FairPriceCalculator,
    binance: &nord::MidPrice,
    zo: Option<nord::MidPrice>,
    last_count: &mut isize,
    target: usize,
    now_ms: u64,
) {
    let state = calc.get_state(now_ms);
    if state.samples as isize == *last_count {
        return;
    }
    *last_count = state.samples as isize;

    let offset_bps = if let Some(offset) = state.offset {
        if binance.mid > 0.0 {
            format!("{:.1}", offset / binance.mid * 10000.0)
        } else {
            "--".into()
        }
    } else {
        "--".into()
    };

    let zo_str = zo
        .map(|p| format!("${:.2}", p.mid))
        .unwrap_or_else(|| "--".into());

    info!(
        samples = format!("{}/{target}", state.samples),
        binance = format!("${:.2}", binance.mid),
        zo = zo_str,
        offset_bps,
        "warming up"
    );
}

fn log_status(tracker: &PositionTracker, orders: &[CachedOrder]) {
    let pos = tracker.get_base_size();
    let bids: Vec<String> = orders
        .iter()
        .filter(|o| o.side == Side::Bid)
        .map(|o| format!("${}x{}", o.price, o.size))
        .collect();
    let asks: Vec<String> = orders
        .iter()
        .filter(|o| o.side == Side::Ask)
        .map(|o| format!("${}x{}", o.price, o.size))
        .collect();
    let bid_str = if bids.is_empty() {
        "-".to_string()
    } else {
        bids.join(",")
    };
    let ask_str = if asks.is_empty() {
        "-".to_string()
    } else {
        asks.join(",")
    };
    info!(
        pos = format!("{pos:.5}"),
        bid = bid_str,
        ask = ask_str,
        "STATUS"
    );
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_binance_symbol() {
        assert_eq!(derive_binance_symbol("BTC-PERP"), "btcusdt");
        assert_eq!(derive_binance_symbol("ETH-PERP"), "ethusdt");
        assert_eq!(derive_binance_symbol("SOL-PERP"), "solusdt");
        assert_eq!(derive_binance_symbol("DOGE-PERP"), "dogeusdt");
    }

    #[test]
    fn test_map_api_orders_to_cached() {
        let api_orders = vec![nord::OpenOrder {
            order_id: 42,
            market_id: 1,
            side: Side::Bid,
            size: 0.5,
            price: 50000.0,
            original_order_size: 0.5,
            client_order_id: None,
        }];
        let cached = map_api_orders_to_cached(&api_orders);
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].order_id, 42);
        assert_eq!(cached[0].side, Side::Bid);
    }
}
