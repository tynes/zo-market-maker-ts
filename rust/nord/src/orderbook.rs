//! Local orderbook maintained via WebSocket deltas and REST snapshots.
//!
//! This module ports the TypeScript `ZoOrderbookStream` to Rust.
//! It spawns a background tokio task that owns all mutable state and pushes
//! price/depth updates to consumers via `tokio::sync::watch` channels
//! (lock-free reads).
//!
//! # Architecture
//!
//! ```text
//!   broadcast::Receiver<WebSocketDeltaUpdate>
//!              |
//!              v
//!   +----- background task (owns OrderbookInner) -----+
//!   |  - fetches REST snapshot on start / lag / stale  |
//!   |  - applies delta updates to BTreeMap-based sides |
//!   |  - computes mid-price & depth after each update  |
//!   +--------------------------------------------------+
//!              |                        |
//!        watch::Sender<MidPrice>  watch::Sender<Depth>
//!              |                        |
//!              v                        v
//!         consumers (zero-cost borrow via watch::Receiver)
//! ```

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use ordered_float::OrderedFloat;
use tokio::sync::{broadcast, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::client::Nord;
use crate::error::Result;
use crate::ws::events::{OrderbookEntry, WebSocketDeltaUpdate};

/// Consider the book stale after 60 s without an update.
const STALE_THRESHOLD_MS: u64 = 60_000;

/// How often to check for staleness.
const STALE_CHECK_INTERVAL_MS: u64 = 10_000;

/// Maximum price levels kept per side to bound memory.
const MAX_LEVELS: usize = 100;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Mid-price derived from best bid and best ask.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MidPrice {
    /// Arithmetic mean of `bid` and `ask`.
    pub mid: f64,
    /// Best bid price.
    pub bid: f64,
    /// Best ask price.
    pub ask: f64,
    /// Unix epoch milliseconds when the price was computed.
    pub timestamp: u64,
}

/// Best bid and best ask prices.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct BBO {
    /// Highest resting bid price.
    pub best_bid: f64,
    /// Lowest resting ask price.
    pub best_ask: f64,
}

/// Snapshot of the full orderbook depth (both sides).
#[derive(Clone, Debug)]
pub struct OrderbookDepth {
    /// Bid levels: price -> size, sorted ascending by price.
    pub bids: BTreeMap<OrderedFloat<f64>, f64>,
    /// Ask levels: price -> size, sorted ascending by price.
    pub asks: BTreeMap<OrderedFloat<f64>, f64>,
}

// ---------------------------------------------------------------------------
// OrderbookSide
// ---------------------------------------------------------------------------

/// One side of the orderbook backed by a `BTreeMap` for O(log n)
/// insert/remove and free sorted iteration. This replaces the TypeScript
/// `Map` + `sortedPrices` array approach, eliminating the O(n log n)
/// rebuild on every structural change.
#[derive(Clone, Debug)]
pub struct OrderbookSide {
    levels: BTreeMap<OrderedFloat<f64>, f64>,
    /// `true` for the ask side, `false` for the bid side.
    /// Determines which end is "best" and which end gets trimmed.
    is_ask: bool,
}

impl OrderbookSide {
    /// Create a new empty side.
    ///
    /// # Arguments
    ///
    /// * `is_ask` - `true` for the ask side (best = lowest price),
    ///   `false` for the bid side (best = highest price).
    pub fn new(is_ask: bool) -> Self {
        Self {
            levels: BTreeMap::new(),
            is_ask,
        }
    }

    /// Apply incremental delta updates. An entry with `size == 0.0` removes
    /// that price level; otherwise the level is inserted or updated.
    /// Trims to [`MAX_LEVELS`] afterwards.
    ///
    /// # Arguments
    ///
    /// * `entries` - Slice of orderbook entries to apply.
    pub fn apply_deltas(&mut self, entries: &[OrderbookEntry]) {
        for entry in entries {
            let key = OrderedFloat(entry.price);
            if entry.size == 0.0 {
                self.levels.remove(&key);
            } else {
                self.levels.insert(key, entry.size);
            }
        }
        self.trim();
    }

    /// Replace all levels with a fresh snapshot. Zero-size entries are
    /// ignored.
    ///
    /// # Arguments
    ///
    /// * `entries` - Slice of orderbook entries forming the new snapshot.
    pub fn set_snapshot(&mut self, entries: &[OrderbookEntry]) {
        self.levels.clear();
        for entry in entries {
            if entry.size > 0.0 {
                self.levels.insert(OrderedFloat(entry.price), entry.size);
            }
        }
        self.trim();
    }

    /// Return the best (top-of-book) price, or `None` if the side is empty.
    ///
    /// - **Asks**: lowest price (first key in ascending BTreeMap).
    /// - **Bids**: highest price (last key in ascending BTreeMap).
    pub fn get_best(&self) -> Option<f64> {
        if self.is_ask {
            // BTreeMap is ascending; first key = lowest price = best ask.
            self.levels.keys().next().map(|k| k.0)
        } else {
            // Last key = highest price = best bid.
            self.levels.keys().next_back().map(|k| k.0)
        }
    }

    /// Remove all levels.
    pub fn clear(&mut self) {
        self.levels.clear();
    }

    /// Number of price levels on this side.
    pub fn len(&self) -> usize {
        self.levels.len()
    }

    /// Whether this side has no levels.
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    /// Clone of all levels (price -> size).
    pub fn get_levels(&self) -> BTreeMap<OrderedFloat<f64>, f64> {
        self.levels.clone()
    }

    /// Trim to `MAX_LEVELS` by removing the worst prices.
    ///
    /// - **Asks**: remove the *highest* prices (worst asks).
    /// - **Bids**: remove the *lowest* prices (worst bids).
    fn trim(&mut self) {
        while self.levels.len() > MAX_LEVELS {
            if self.is_ask {
                // Remove highest (worst ask).
                self.levels.pop_last();
            } else {
                // Remove lowest (worst bid).
                self.levels.pop_first();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal state for the background task
// ---------------------------------------------------------------------------

/// Mutable state owned exclusively by the background task.
struct OrderbookInner {
    bids: OrderbookSide,
    asks: OrderbookSide,
    last_update_id: u64,
    last_update_time: u64,
    snapshot_loaded: bool,
    delta_buffer: Vec<WebSocketDeltaUpdate>,
}

impl OrderbookInner {
    fn new() -> Self {
        Self {
            bids: OrderbookSide::new(false),
            asks: OrderbookSide::new(true),
            last_update_id: 0,
            last_update_time: 0,
            snapshot_loaded: false,
            delta_buffer: Vec::new(),
        }
    }

    /// Reset all state (used before re-fetching a snapshot).
    fn reset(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.last_update_id = 0;
        self.last_update_time = 0;
        self.snapshot_loaded = false;
        self.delta_buffer.clear();
    }
}

// ---------------------------------------------------------------------------
// OrderbookStream
// ---------------------------------------------------------------------------

/// Manages a live local orderbook for a single market symbol.
///
/// Call [`OrderbookStream::new`] then [`OrderbookStream::connect`] to start.
/// Read the latest price with [`OrderbookStream::get_mid_price`] /
/// [`OrderbookStream::get_bbo`], or clone a `watch::Receiver` via
/// [`OrderbookStream::subscribe_price`] / [`OrderbookStream::subscribe_depth`]
/// for async consumption.
pub struct OrderbookStream {
    symbol: String,
    nord: Nord,
    /// Receiver end of the broadcast channel for delta updates.
    /// `Some` before `connect()`, `None` after (moved into the task).
    delta_rx: Option<broadcast::Receiver<WebSocketDeltaUpdate>>,
    // Watch channels: background task sends, consumers borrow.
    price_tx: watch::Sender<Option<MidPrice>>,
    price_rx: watch::Receiver<Option<MidPrice>>,
    depth_tx: watch::Sender<Option<OrderbookDepth>>,
    depth_rx: watch::Receiver<Option<OrderbookDepth>>,
    task_handle: Option<JoinHandle<()>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl OrderbookStream {
    /// Create a new `OrderbookStream`. Call [`connect`](Self::connect) to
    /// start the background processing task.
    ///
    /// # Arguments
    ///
    /// * `symbol` - Market symbol, e.g. `"BTC-PERP"`.
    /// * `nord` - An initialised [`Nord`] client (used for REST snapshots).
    /// * `delta_rx` - Broadcast receiver from the WebSocket client's
    ///   `subscribe_deltas()` method.
    pub fn new(
        symbol: String,
        nord: Nord,
        delta_rx: broadcast::Receiver<WebSocketDeltaUpdate>,
    ) -> Self {
        let (price_tx, price_rx) = watch::channel(None);
        let (depth_tx, depth_rx) = watch::channel(None);

        Self {
            symbol,
            nord,
            delta_rx: Some(delta_rx),
            price_tx,
            price_rx,
            depth_tx,
            depth_rx,
            task_handle: None,
            shutdown_tx: None,
        }
    }

    /// Start the background task that maintains the local orderbook.
    ///
    /// Fetches an initial REST snapshot, drains any buffered WebSocket
    /// deltas, then enters the main event loop.
    ///
    /// # Errors
    ///
    /// Returns an error if the initial REST snapshot fetch fails or if
    /// `connect` has already been called (delta_rx consumed).
    pub async fn connect(&mut self) -> Result<()> {
        let delta_rx = self
            .delta_rx
            .take()
            .expect("connect() called twice: delta_rx already consumed");

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        // oneshot for the background task to signal that the initial
        // snapshot has loaded (or that it failed).
        let (ready_tx, ready_rx) = oneshot::channel::<Result<()>>();

        let symbol = self.symbol.clone();
        let nord = self.nord.clone();
        let price_tx = self.price_tx.clone();
        let depth_tx = self.depth_tx.clone();

        let handle = tokio::spawn(async move {
            run_background_task(
                symbol,
                nord,
                delta_rx,
                shutdown_rx,
                price_tx,
                depth_tx,
                ready_tx,
            )
            .await;
        });

        self.task_handle = Some(handle);

        // Wait for the initial snapshot to succeed (or fail).
        match ready_rx.await {
            Ok(result) => result,
            // The task dropped ready_tx without sending -- treat as error.
            Err(_) => Err(crate::error::NordError::WebSocket(
                "orderbook background task exited before ready".into(),
            )),
        }
    }

    /// Latest computed mid-price, or `None` if no valid BBO exists yet.
    pub fn get_mid_price(&self) -> Option<MidPrice> {
        *self.price_rx.borrow()
    }

    /// Latest best-bid-offer, or `None` if no valid BBO exists yet.
    pub fn get_bbo(&self) -> Option<BBO> {
        self.get_mid_price().map(|p| BBO {
            best_bid: p.bid,
            best_ask: p.ask,
        })
    }

    /// Clone a `watch::Receiver` for async price consumption.
    /// Call `.changed().await` to wait for the next update, then
    /// `.borrow()` to read the value.
    pub fn subscribe_price(&self) -> watch::Receiver<Option<MidPrice>> {
        self.price_rx.clone()
    }

    /// Clone a `watch::Receiver` for async depth consumption.
    pub fn subscribe_depth(&self) -> watch::Receiver<Option<OrderbookDepth>> {
        self.depth_rx.clone()
    }

    /// Shut down the background task and release resources.
    pub fn close(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
    }
}

impl Drop for OrderbookStream {
    fn drop(&mut self) {
        self.close();
    }
}

// ---------------------------------------------------------------------------
// Background task
// ---------------------------------------------------------------------------

/// The long-running background task that owns all mutable orderbook state.
async fn run_background_task(
    symbol: String,
    nord: Nord,
    mut delta_rx: broadcast::Receiver<WebSocketDeltaUpdate>,
    mut shutdown_rx: oneshot::Receiver<()>,
    price_tx: watch::Sender<Option<MidPrice>>,
    depth_tx: watch::Sender<Option<OrderbookDepth>>,
    ready_tx: oneshot::Sender<Result<()>>,
) {
    let mut inner = OrderbookInner::new();

    // 1. Fetch initial REST snapshot.
    if let Err(e) = fetch_snapshot(&nord, &symbol, &mut inner).await {
        error!("initial orderbook snapshot failed for {symbol}: {e}");
        let _ = ready_tx.send(Err(e));
        return;
    }

    // 2. Drain buffered deltas received while the snapshot was in flight.
    drain_buffered_deltas(&mut delta_rx, &mut inner);

    // 3. Emit initial price.
    emit_price(&inner, &price_tx, &depth_tx);

    info!(
        "orderbook active for {symbol} ({} bids, {} asks, update_id={})",
        inner.bids.len(),
        inner.asks.len(),
        inner.last_update_id,
    );

    // Signal readiness to the caller.
    let _ = ready_tx.send(Ok(()));

    // 4. Main event loop.
    let mut stale_interval =
        tokio::time::interval(std::time::Duration::from_millis(STALE_CHECK_INTERVAL_MS));

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                debug!("orderbook shutdown for {symbol}");
                break;
            }
            _ = stale_interval.tick() => {
                let now = epoch_millis();
                if inner.last_update_time > 0
                    && now.saturating_sub(inner.last_update_time) > STALE_THRESHOLD_MS
                {
                    warn!(
                        "orderbook stale for {symbol} ({}ms since last update), re-fetching",
                        now.saturating_sub(inner.last_update_time),
                    );
                    inner.reset();
                    if let Err(e) = fetch_snapshot(&nord, &symbol, &mut inner).await {
                        error!("snapshot re-fetch failed for {symbol}: {e}");
                        continue;
                    }
                    drain_buffered_deltas(&mut delta_rx, &mut inner);
                    emit_price(&inner, &price_tx, &depth_tx);
                }
            }
            result = delta_rx.recv() => {
                match result {
                    Ok(update) => {
                        inner.last_update_time = epoch_millis();

                        if !inner.snapshot_loaded {
                            inner.delta_buffer.push(update);
                            continue;
                        }

                        // Filter by market symbol.
                        if update.market_symbol != symbol {
                            continue;
                        }

                        // Skip stale updates.
                        if update.update_id <= inner.last_update_id {
                            continue;
                        }

                        handle_update(&mut inner, &update);
                        emit_price(&inner, &price_tx, &depth_tx);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(
                            "orderbook delta lagged by {n} for {symbol}, re-fetching snapshot"
                        );
                        inner.reset();
                        if let Err(e) = fetch_snapshot(&nord, &symbol, &mut inner).await {
                            error!("snapshot re-fetch after lag failed for {symbol}: {e}");
                            continue;
                        }
                        drain_buffered_deltas(&mut delta_rx, &mut inner);
                        emit_price(&inner, &price_tx, &depth_tx);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("delta channel closed for {symbol}, exiting");
                        break;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Current Unix epoch in milliseconds.
fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Fetch a REST snapshot and populate `inner`.
async fn fetch_snapshot(nord: &Nord, symbol: &str, inner: &mut OrderbookInner) -> Result<()> {
    debug!("fetching orderbook snapshot for {symbol}");
    let info = nord.get_orderbook_by_symbol(symbol).await?;

    // Convert `[price, size]` arrays to `OrderbookEntry`.
    let bid_entries: Vec<OrderbookEntry> = info
        .bids
        .iter()
        .map(|pair| OrderbookEntry {
            price: pair[0],
            size: pair[1],
        })
        .collect();
    let ask_entries: Vec<OrderbookEntry> = info
        .asks
        .iter()
        .map(|pair| OrderbookEntry {
            price: pair[0],
            size: pair[1],
        })
        .collect();

    inner.bids.set_snapshot(&bid_entries);
    inner.asks.set_snapshot(&ask_entries);
    inner.last_update_id = info.update_id;
    inner.last_update_time = epoch_millis();
    inner.snapshot_loaded = true;

    info!(
        "orderbook snapshot loaded for {symbol} (update_id={}, {} bids, {} asks)",
        info.update_id,
        inner.bids.len(),
        inner.asks.len(),
    );

    Ok(())
}

/// Drain any buffered deltas from the broadcast channel and apply those
/// whose `update_id` is newer than the current snapshot.
fn drain_buffered_deltas(
    delta_rx: &mut broadcast::Receiver<WebSocketDeltaUpdate>,
    inner: &mut OrderbookInner,
) {
    let mut applied = 0u64;
    let mut skipped = 0u64;

    // Drain any messages that arrived while the snapshot was in flight.
    loop {
        match delta_rx.try_recv() {
            Ok(update) => {
                if update.update_id > inner.last_update_id {
                    handle_update(inner, &update);
                    applied += 1;
                } else {
                    skipped += 1;
                }
            }
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                warn!("lagged by {n} during buffered drain, continuing");
            }
            Err(broadcast::error::TryRecvError::Closed) => break,
        }
    }

    // Also drain the in-memory buffer (deltas received before snapshot
    // was marked loaded).
    for update in inner.delta_buffer.drain(..) {
        if update.update_id > inner.last_update_id {
            inner.bids.apply_deltas(&update.bids);
            inner.asks.apply_deltas(&update.asks);
            inner.last_update_id = update.update_id;
            applied += 1;
        } else {
            skipped += 1;
        }
    }

    if applied > 0 || skipped > 0 {
        info!("buffered deltas: applied {applied}, skipped {skipped}");
    }
}

/// Apply a single delta update to the inner state.
fn handle_update(inner: &mut OrderbookInner, update: &WebSocketDeltaUpdate) {
    inner.bids.apply_deltas(&update.bids);
    inner.asks.apply_deltas(&update.asks);
    inner.last_update_id = update.update_id;
}

/// Compute mid-price from best bid/ask and send to watch channels.
fn emit_price(
    inner: &OrderbookInner,
    price_tx: &watch::Sender<Option<MidPrice>>,
    depth_tx: &watch::Sender<Option<OrderbookDepth>>,
) {
    let best_bid = inner.bids.get_best();
    let best_ask = inner.asks.get_best();

    if let (Some(bid), Some(ask)) = (best_bid, best_ask) {
        let mid = (bid + ask) / 2.0;
        let price = MidPrice {
            mid,
            bid,
            ask,
            timestamp: epoch_millis(),
        };
        let _ = price_tx.send(Some(price));
    }

    // Always emit depth so consumers see the current state.
    let _ = depth_tx.send(Some(OrderbookDepth {
        bids: inner.bids.get_levels(),
        asks: inner.asks.get_levels(),
    }));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- OrderbookSide: ask side -----------------------------------------

    #[test]
    fn ask_best_is_lowest_price() {
        let mut side = OrderbookSide::new(true);
        side.apply_deltas(&[
            OrderbookEntry {
                price: 105.0,
                size: 1.0,
            },
            OrderbookEntry {
                price: 100.0,
                size: 2.0,
            },
            OrderbookEntry {
                price: 110.0,
                size: 3.0,
            },
        ]);
        assert_eq!(side.get_best(), Some(100.0));
    }

    #[test]
    fn bid_best_is_highest_price() {
        let mut side = OrderbookSide::new(false);
        side.apply_deltas(&[
            OrderbookEntry {
                price: 95.0,
                size: 1.0,
            },
            OrderbookEntry {
                price: 100.0,
                size: 2.0,
            },
            OrderbookEntry {
                price: 90.0,
                size: 3.0,
            },
        ]);
        assert_eq!(side.get_best(), Some(100.0));
    }

    // -- apply_deltas: insert, update, remove ----------------------------

    #[test]
    fn apply_deltas_inserts_new_levels() {
        let mut side = OrderbookSide::new(true);
        assert_eq!(side.len(), 0);

        side.apply_deltas(&[
            OrderbookEntry {
                price: 100.0,
                size: 5.0,
            },
            OrderbookEntry {
                price: 101.0,
                size: 3.0,
            },
        ]);
        assert_eq!(side.len(), 2);
    }

    #[test]
    fn apply_deltas_updates_existing_level() {
        let mut side = OrderbookSide::new(true);
        side.apply_deltas(&[OrderbookEntry {
            price: 100.0,
            size: 5.0,
        }]);
        assert_eq!(side.levels.get(&OrderedFloat(100.0)), Some(&5.0));

        side.apply_deltas(&[OrderbookEntry {
            price: 100.0,
            size: 10.0,
        }]);
        assert_eq!(side.levels.get(&OrderedFloat(100.0)), Some(&10.0));
        assert_eq!(side.len(), 1);
    }

    #[test]
    fn apply_deltas_removes_on_zero_size() {
        let mut side = OrderbookSide::new(true);
        side.apply_deltas(&[
            OrderbookEntry {
                price: 100.0,
                size: 5.0,
            },
            OrderbookEntry {
                price: 101.0,
                size: 3.0,
            },
        ]);
        assert_eq!(side.len(), 2);

        side.apply_deltas(&[OrderbookEntry {
            price: 100.0,
            size: 0.0,
        }]);
        assert_eq!(side.len(), 1);
        assert!(side.levels.get(&OrderedFloat(100.0)).is_none());
    }

    // -- set_snapshot ----------------------------------------------------

    #[test]
    fn set_snapshot_replaces_all_levels() {
        let mut side = OrderbookSide::new(true);
        side.apply_deltas(&[
            OrderbookEntry {
                price: 100.0,
                size: 5.0,
            },
            OrderbookEntry {
                price: 101.0,
                size: 3.0,
            },
        ]);
        assert_eq!(side.len(), 2);

        side.set_snapshot(&[OrderbookEntry {
            price: 200.0,
            size: 1.0,
        }]);
        assert_eq!(side.len(), 1);
        assert_eq!(side.get_best(), Some(200.0));
    }

    #[test]
    fn set_snapshot_ignores_zero_size_entries() {
        let mut side = OrderbookSide::new(true);
        side.set_snapshot(&[
            OrderbookEntry {
                price: 100.0,
                size: 5.0,
            },
            OrderbookEntry {
                price: 101.0,
                size: 0.0,
            },
            OrderbookEntry {
                price: 102.0,
                size: 2.0,
            },
        ]);
        assert_eq!(side.len(), 2);
        assert!(side.levels.get(&OrderedFloat(101.0)).is_none());
    }

    // -- Trimming --------------------------------------------------------

    #[test]
    fn asks_trim_removes_highest_prices() {
        let mut side = OrderbookSide::new(true);
        // Insert MAX_LEVELS + 5 levels.
        let entries: Vec<OrderbookEntry> = (0..MAX_LEVELS + 5)
            .map(|i| OrderbookEntry {
                price: 100.0 + i as f64,
                size: 1.0,
            })
            .collect();
        side.apply_deltas(&entries);

        assert_eq!(side.len(), MAX_LEVELS);
        // Best ask = lowest = 100.0 (still present).
        assert_eq!(side.get_best(), Some(100.0));
        // Highest 5 prices should have been removed.
        for i in 0..5 {
            let price = 100.0 + (MAX_LEVELS + i) as f64;
            assert!(
                side.levels.get(&OrderedFloat(price)).is_none(),
                "price {price} should have been trimmed"
            );
        }
    }

    #[test]
    fn bids_trim_removes_lowest_prices() {
        let mut side = OrderbookSide::new(false);
        let entries: Vec<OrderbookEntry> = (0..MAX_LEVELS + 5)
            .map(|i| OrderbookEntry {
                price: 100.0 + i as f64,
                size: 1.0,
            })
            .collect();
        side.apply_deltas(&entries);

        assert_eq!(side.len(), MAX_LEVELS);
        // Best bid = highest = 100.0 + (MAX_LEVELS + 4) (still present).
        let highest = 100.0 + (MAX_LEVELS + 4) as f64;
        assert_eq!(side.get_best(), Some(highest));
        // Lowest 5 prices should have been removed.
        for i in 0..5 {
            let price = 100.0 + i as f64;
            assert!(
                side.levels.get(&OrderedFloat(price)).is_none(),
                "price {price} should have been trimmed"
            );
        }
    }

    // -- get_best on empty -----------------------------------------------

    #[test]
    fn get_best_returns_none_when_empty() {
        let ask_side = OrderbookSide::new(true);
        assert_eq!(ask_side.get_best(), None);

        let bid_side = OrderbookSide::new(false);
        assert_eq!(bid_side.get_best(), None);
    }

    // -- clear -----------------------------------------------------------

    #[test]
    fn clear_resets_everything() {
        let mut side = OrderbookSide::new(true);
        side.apply_deltas(&[
            OrderbookEntry {
                price: 100.0,
                size: 5.0,
            },
            OrderbookEntry {
                price: 101.0,
                size: 3.0,
            },
        ]);
        assert_eq!(side.len(), 2);

        side.clear();
        assert_eq!(side.len(), 0);
        assert!(side.is_empty());
        assert_eq!(side.get_best(), None);
    }

    // -- len / is_empty --------------------------------------------------

    #[test]
    fn len_tracks_count() {
        let mut side = OrderbookSide::new(true);
        assert_eq!(side.len(), 0);
        assert!(side.is_empty());

        side.apply_deltas(&[OrderbookEntry {
            price: 100.0,
            size: 1.0,
        }]);
        assert_eq!(side.len(), 1);
        assert!(!side.is_empty());

        side.apply_deltas(&[OrderbookEntry {
            price: 101.0,
            size: 1.0,
        }]);
        assert_eq!(side.len(), 2);

        // Remove one.
        side.apply_deltas(&[OrderbookEntry {
            price: 100.0,
            size: 0.0,
        }]);
        assert_eq!(side.len(), 1);
    }

    // -- get_levels returns a clone --------------------------------------

    #[test]
    fn get_levels_returns_independent_clone() {
        let mut side = OrderbookSide::new(true);
        side.apply_deltas(&[
            OrderbookEntry {
                price: 100.0,
                size: 5.0,
            },
            OrderbookEntry {
                price: 101.0,
                size: 3.0,
            },
        ]);

        let levels = side.get_levels();
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[&OrderedFloat(100.0)], 5.0);
        assert_eq!(levels[&OrderedFloat(101.0)], 3.0);

        // Mutating the side does not affect the returned map.
        side.clear();
        assert_eq!(levels.len(), 2);
    }
}
