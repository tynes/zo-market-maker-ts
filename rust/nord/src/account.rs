//! Live account order/fill tracking via WebSocket.
//!
//! Mirrors the TypeScript `AccountStream` class, ported to Rust following the
//! same background-task pattern as [`crate::orderbook::OrderbookStream`].
//!
//! # Architecture
//!
//! A background tokio task owns a `HashMap<u64, TrackedOrder>` and applies
//! WebSocket `places`, `fills`, and `cancels` events. Fill events are forwarded
//! to an `mpsc` channel so the caller can react (e.g. update position).
//! The current orders map is published via a `watch` channel for lock-free
//! reads.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::client::Nord;
use crate::types::Side;
use crate::ws::events::WebSocketAccountUpdate;

/// Reconnect delay after the WebSocket feed drops.
const RECONNECT_DELAY_MS: u64 = 3000;

/// An open order tracked from WebSocket events.
#[derive(Debug, Clone)]
pub struct TrackedOrder {
    pub order_id: u64,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub market_id: u32,
}

/// A fill event emitted when an order is (partially) filled.
#[derive(Debug, Clone)]
pub struct FillEvent {
    pub order_id: u64,
    pub side: Side,
    pub size: f64,
    pub price: f64,
    pub remaining: f64,
    pub market_id: u32,
}

/// Live account stream — tracks orders/fills/cancels in real-time.
///
/// Call [`AccountStream::new`] then [`AccountStream::connect`] to start.
pub struct AccountStream {
    account_id: u32,
    account_rx: Option<broadcast::Receiver<WebSocketAccountUpdate>>,
    orders_rx: watch::Receiver<HashMap<u64, TrackedOrder>>,
    orders_tx: watch::Sender<HashMap<u64, TrackedOrder>>,
    fill_rx: Option<mpsc::UnboundedReceiver<FillEvent>>,
    fill_tx: mpsc::UnboundedSender<FillEvent>,
    cancel: CancellationToken,
    task_handle: Option<JoinHandle<()>>,
    nord: Option<Arc<Nord>>,
}

impl AccountStream {
    /// Create a new `AccountStream`.
    ///
    /// # Arguments
    ///
    /// * `account_id` - Exchange account to track.
    /// * `account_rx` - Broadcast receiver from the WebSocket client's
    ///   `subscribe_accounts()` method.
    /// * `nord` - Shared Nord client for REST re-sync on reconnect.
    pub fn new(
        account_id: u32,
        account_rx: broadcast::Receiver<WebSocketAccountUpdate>,
        nord: Arc<Nord>,
    ) -> Self {
        let (orders_tx, orders_rx) = watch::channel(HashMap::new());
        let (fill_tx, fill_rx) = mpsc::unbounded_channel();
        Self {
            account_id,
            account_rx: Some(account_rx),
            orders_rx,
            orders_tx,
            fill_rx: Some(fill_rx),
            fill_tx,
            cancel: CancellationToken::new(),
            task_handle: None,
            nord: Some(nord),
        }
    }

    /// Start the background processing task.
    pub fn connect(&mut self) {
        let account_rx = self
            .account_rx
            .take()
            .expect("connect() called twice: account_rx already consumed");

        let account_id = self.account_id;
        let orders_tx = self.orders_tx.clone();
        let fill_tx = self.fill_tx.clone();
        let cancel = self.cancel.clone();
        let nord = self
            .nord
            .take()
            .expect("connect() called twice: nord already consumed");

        info!(account_id, "subscribing to account updates");

        let handle = tokio::spawn(async move {
            run_account_task(account_id, account_rx, orders_tx, fill_tx, cancel, nord).await;
        });
        self.task_handle = Some(handle);
    }

    /// Take the fill event receiver (can only be called once).
    pub fn take_fill_rx(&mut self) -> Option<mpsc::UnboundedReceiver<FillEvent>> {
        self.fill_rx.take()
    }

    /// Current orders map (lock-free read).
    pub fn get_orders(&self) -> HashMap<u64, TrackedOrder> {
        self.orders_rx.borrow().clone()
    }

    /// Orders filtered by market ID.
    pub fn get_orders_for_market(&self, market_id: u32) -> Vec<TrackedOrder> {
        self.orders_rx
            .borrow()
            .values()
            .filter(|o| o.market_id == market_id)
            .cloned()
            .collect()
    }

    /// Subscribe to order map changes.
    pub fn subscribe_orders(&self) -> watch::Receiver<HashMap<u64, TrackedOrder>> {
        self.orders_rx.clone()
    }

    /// Sync initial order state from the server (call after `user.fetch_info()`).
    ///
    /// Replaces the current orders map with orders from the given slice.
    pub fn sync_initial_orders(&self, orders: &[crate::types::OpenOrder]) {
        let map: HashMap<u64, TrackedOrder> = orders
            .iter()
            .map(|o| {
                (
                    o.order_id,
                    TrackedOrder {
                        order_id: o.order_id,
                        side: o.side,
                        price: o.price,
                        size: o.size,
                        market_id: o.market_id,
                    },
                )
            })
            .collect();
        let _ = self.orders_tx.send(map);
    }

    /// Shut down the background task.
    pub fn close(&self) {
        self.cancel.cancel();
    }
}

impl Drop for AccountStream {
    fn drop(&mut self) {
        self.close();
        if let Some(h) = self.task_handle.take() {
            h.abort();
        }
    }
}

// ---------------------------------------------------------------------------
// Background task
// ---------------------------------------------------------------------------

async fn run_account_task(
    account_id: u32,
    mut account_rx: broadcast::Receiver<WebSocketAccountUpdate>,
    orders_tx: watch::Sender<HashMap<u64, TrackedOrder>>,
    fill_tx: mpsc::UnboundedSender<FillEvent>,
    cancel: CancellationToken,
    nord: Arc<Nord>,
) {
    info!(account_id, "account stream active");

    loop {
        tokio::select! {
            update = account_rx.recv() => {
                match update {
                    Ok(data) => {
                        if data.account_id != account_id {
                            continue;
                        }
                        apply_update(&data, &orders_tx, &fill_tx);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(account_id, skipped = n, "account stream lagged — re-syncing");
                        resync_orders(&nord, account_id, &orders_tx).await;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        warn!(account_id, "account broadcast channel closed");
                        // Wait and hope for reconnect upstream.
                        tokio::select! {
                            _ = time::sleep(Duration::from_millis(RECONNECT_DELAY_MS)) => {}
                            _ = cancel.cancelled() => break,
                        }
                    }
                }
            }
            _ = cancel.cancelled() => {
                info!(account_id, "account stream shutting down");
                break;
            }
        }
    }
}

/// Apply a single WebSocket account update to the orders map.
fn apply_update(
    data: &WebSocketAccountUpdate,
    orders_tx: &watch::Sender<HashMap<u64, TrackedOrder>>,
    fill_tx: &mpsc::UnboundedSender<FillEvent>,
) {
    orders_tx.send_modify(|orders| {
        // Placements
        for (id_str, place) in &data.places {
            if let Ok(order_id) = id_str.parse::<u64>() {
                orders.insert(
                    order_id,
                    TrackedOrder {
                        order_id,
                        side: place.side,
                        price: place.price,
                        size: place.current_size,
                        market_id: place.market_id,
                    },
                );
            }
        }

        // Fills
        for (id_str, fill) in &data.fills {
            if let Ok(order_id) = id_str.parse::<u64>() {
                if fill.quantity > 0.0 {
                    let _ = fill_tx.send(FillEvent {
                        order_id,
                        side: fill.side,
                        size: fill.quantity,
                        price: fill.price,
                        remaining: fill.remaining,
                        market_id: fill.market_id,
                    });
                }

                if fill.remaining <= 0.0 {
                    orders.remove(&order_id);
                } else if let Some(existing) = orders.get_mut(&order_id) {
                    existing.size = fill.remaining;
                }
            }
        }

        // Cancellations
        for id_str in data.cancels.keys() {
            if let Ok(order_id) = id_str.parse::<u64>() {
                orders.remove(&order_id);
            }
        }
    });
}

/// Re-fetch orders from the server after a lag event.
async fn resync_orders(
    nord: &Nord,
    account_id: u32,
    orders_tx: &watch::Sender<HashMap<u64, TrackedOrder>>,
) {
    match nord.get_account(account_id).await {
        Ok(account) => {
            let map: HashMap<u64, TrackedOrder> = account
                .orders
                .iter()
                .map(|o| {
                    (
                        o.order_id,
                        TrackedOrder {
                            order_id: o.order_id,
                            side: o.side,
                            price: o.price,
                            size: o.size,
                            market_id: o.market_id,
                        },
                    )
                })
                .collect();
            info!(
                account_id,
                orders = map.len(),
                "re-synced orders from server"
            );
            let _ = orders_tx.send(map);
        }
        Err(e) => {
            error!(account_id, error = %e, "failed to re-sync orders");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ws::events::{AccountCancel, AccountFill, AccountPlace};

    /// Helper: build a WebSocketAccountUpdate from places/fills/cancels.
    fn make_update(
        places: HashMap<String, AccountPlace>,
        fills: HashMap<String, AccountFill>,
        cancels: HashMap<String, AccountCancel>,
    ) -> WebSocketAccountUpdate {
        WebSocketAccountUpdate {
            last_update_id: 0,
            update_id: 1,
            account_id: 1,
            fills,
            places,
            cancels,
            balances: HashMap::new(),
        }
    }

    fn orders_and_fill() -> (
        watch::Sender<HashMap<u64, TrackedOrder>>,
        watch::Receiver<HashMap<u64, TrackedOrder>>,
        mpsc::UnboundedSender<FillEvent>,
        mpsc::UnboundedReceiver<FillEvent>,
    ) {
        let (otx, orx) = watch::channel(HashMap::new());
        let (ftx, frx) = mpsc::unbounded_channel();
        (otx, orx, ftx, frx)
    }

    #[test]
    fn test_place_event_adds_tracked_order() {
        let (otx, orx, ftx, _frx) = orders_and_fill();
        let mut places = HashMap::new();
        places.insert(
            "100".to_string(),
            AccountPlace {
                side: Side::Bid,
                current_size: 1.5,
                price: 50000.0,
                market_id: 1,
            },
        );
        let update = make_update(places, HashMap::new(), HashMap::new());
        apply_update(&update, &otx, &ftx);

        let orders = orx.borrow();
        assert_eq!(orders.len(), 1);
        let order = orders.get(&100).unwrap();
        assert_eq!(order.order_id, 100);
        assert_eq!(order.side, Side::Bid);
        assert!((order.price - 50000.0).abs() < 1e-6);
        assert!((order.size - 1.5).abs() < 1e-6);
    }

    #[test]
    fn test_fill_event_emits_fill_and_updates_size() {
        let (otx, orx, ftx, mut frx) = orders_and_fill();

        // First: place an order.
        let mut places = HashMap::new();
        places.insert(
            "200".to_string(),
            AccountPlace {
                side: Side::Ask,
                current_size: 2.0,
                price: 51000.0,
                market_id: 1,
            },
        );
        apply_update(
            &make_update(places, HashMap::new(), HashMap::new()),
            &otx,
            &ftx,
        );

        // Now: partial fill (remaining > 0).
        let mut fills = HashMap::new();
        fills.insert(
            "200".to_string(),
            AccountFill {
                side: Side::Ask,
                quantity: 0.5,
                remaining: 1.5,
                price: 51000.0,
                order_id: "200".to_string(),
                market_id: 1,
                maker_id: 1,
                taker_id: 2,
                sender_tracking_id: None,
            },
        );
        apply_update(
            &make_update(HashMap::new(), fills, HashMap::new()),
            &otx,
            &ftx,
        );

        // Check fill was emitted.
        let fill = frx.try_recv().unwrap();
        assert_eq!(fill.order_id, 200);
        assert!((fill.size - 0.5).abs() < 1e-6);
        assert!((fill.remaining - 1.5).abs() < 1e-6);

        // Check order size was updated.
        let orders = orx.borrow();
        let order = orders.get(&200).unwrap();
        assert!((order.size - 1.5).abs() < 1e-6);
    }

    #[test]
    fn test_fill_removes_order_when_remaining_zero() {
        let (otx, orx, ftx, _frx) = orders_and_fill();

        // Place.
        let mut places = HashMap::new();
        places.insert(
            "300".to_string(),
            AccountPlace {
                side: Side::Bid,
                current_size: 1.0,
                price: 49000.0,
                market_id: 1,
            },
        );
        apply_update(
            &make_update(places, HashMap::new(), HashMap::new()),
            &otx,
            &ftx,
        );
        assert_eq!(orx.borrow().len(), 1);

        // Full fill (remaining = 0).
        let mut fills = HashMap::new();
        fills.insert(
            "300".to_string(),
            AccountFill {
                side: Side::Bid,
                quantity: 1.0,
                remaining: 0.0,
                price: 49000.0,
                order_id: "300".to_string(),
                market_id: 1,
                maker_id: 1,
                taker_id: 2,
                sender_tracking_id: None,
            },
        );
        apply_update(
            &make_update(HashMap::new(), fills, HashMap::new()),
            &otx,
            &ftx,
        );
        assert!(orx.borrow().is_empty());
    }

    #[test]
    fn test_cancel_event_removes_order() {
        let (otx, orx, ftx, _frx) = orders_and_fill();

        // Place.
        let mut places = HashMap::new();
        places.insert(
            "400".to_string(),
            AccountPlace {
                side: Side::Ask,
                current_size: 0.5,
                price: 52000.0,
                market_id: 2,
            },
        );
        apply_update(
            &make_update(places, HashMap::new(), HashMap::new()),
            &otx,
            &ftx,
        );
        assert_eq!(orx.borrow().len(), 1);

        // Cancel.
        let mut cancels = HashMap::new();
        cancels.insert(
            "400".to_string(),
            AccountCancel {
                side: Side::Ask,
                current_size: 0.5,
                price: 52000.0,
                market_id: 2,
            },
        );
        apply_update(
            &make_update(HashMap::new(), HashMap::new(), cancels),
            &otx,
            &ftx,
        );
        assert!(orx.borrow().is_empty());
    }

    #[test]
    fn test_mixed_update_places_fills_cancels() {
        let (otx, orx, ftx, mut frx) = orders_and_fill();

        // Setup: two existing orders.
        let mut places = HashMap::new();
        places.insert(
            "500".to_string(),
            AccountPlace {
                side: Side::Bid,
                current_size: 1.0,
                price: 50000.0,
                market_id: 1,
            },
        );
        places.insert(
            "501".to_string(),
            AccountPlace {
                side: Side::Ask,
                current_size: 1.0,
                price: 51000.0,
                market_id: 1,
            },
        );
        apply_update(
            &make_update(places, HashMap::new(), HashMap::new()),
            &otx,
            &ftx,
        );
        assert_eq!(orx.borrow().len(), 2);

        // Mixed update: new placement + partial fill on 500 + cancel 501.
        let mut new_places = HashMap::new();
        new_places.insert(
            "502".to_string(),
            AccountPlace {
                side: Side::Ask,
                current_size: 2.0,
                price: 52000.0,
                market_id: 1,
            },
        );
        let mut fills = HashMap::new();
        fills.insert(
            "500".to_string(),
            AccountFill {
                side: Side::Bid,
                quantity: 0.3,
                remaining: 0.7,
                price: 50000.0,
                order_id: "500".to_string(),
                market_id: 1,
                maker_id: 1,
                taker_id: 2,
                sender_tracking_id: None,
            },
        );
        let mut cancels = HashMap::new();
        cancels.insert(
            "501".to_string(),
            AccountCancel {
                side: Side::Ask,
                current_size: 1.0,
                price: 51000.0,
                market_id: 1,
            },
        );
        apply_update(&make_update(new_places, fills, cancels), &otx, &ftx);

        let orders = orx.borrow();
        // 500 updated (size=0.7), 501 cancelled, 502 added → 2 orders.
        assert_eq!(orders.len(), 2);
        assert!(orders.contains_key(&500));
        assert!(orders.contains_key(&502));
        assert!(!orders.contains_key(&501));

        let o500 = orders.get(&500).unwrap();
        assert!((o500.size - 0.7).abs() < 1e-6);

        // Fill event should be available.
        let fill = frx.try_recv().unwrap();
        assert_eq!(fill.order_id, 500);
        assert!((fill.size - 0.3).abs() < 1e-6);
    }
}
