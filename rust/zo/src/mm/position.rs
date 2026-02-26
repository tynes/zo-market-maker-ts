//! Position tracker with optimistic fill updates and periodic server sync.
//!
//! Uses `AtomicU64` (storing `f64` via `to_bits`/`from_bits`) for lock-free
//! reads of the current base position size. A background tokio task
//! periodically fetches the authoritative position from the server and
//! corrects any drift.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use nord::Side;
use tokio::time::{self, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

/// Configuration for the position tracker.
#[derive(Debug, Clone)]
pub struct PositionConfig {
    /// Position USD value that triggers close (reduce-only) mode.
    pub close_threshold_usd: f64,
    /// Interval between server syncs in milliseconds.
    pub sync_interval_ms: u64,
}

/// Snapshot of current position state.
#[derive(Debug, Clone)]
pub struct PositionState {
    /// Signed base-asset size (positive = long, negative = short).
    pub size_base: f64,
    /// Position value in USD (`size_base * fair_price`).
    pub size_usd: f64,
    /// Whether the position is net long.
    pub is_long: bool,
    /// Whether position USD exceeds the close threshold.
    pub is_close_mode: bool,
}

/// Context passed to the quoter for computing quotes.
#[derive(Debug, Clone)]
pub struct QuotingContext {
    /// Current fair price used for quoting.
    pub fair_price: f64,
    /// Current position state snapshot.
    pub position_state: PositionState,
    /// Which sides the quoter is allowed to quote.
    pub allowed_sides: Vec<Side>,
}

/// Lock-free position tracker.
///
/// The `base_size` field is stored as `AtomicU64` using the bit-pattern of
/// the underlying `f64`. This allows concurrent optimistic updates from fill
/// events without taking a mutex.
pub struct PositionTracker {
    config: PositionConfig,
    /// Signed base-asset position (f64 stored as u64 bits).
    base_size: Arc<AtomicU64>,
}

impl PositionTracker {
    /// Create a new tracker (position starts at zero).
    pub fn new(config: PositionConfig) -> Self {
        Self {
            config,
            base_size: Arc::new(AtomicU64::new(0f64.to_bits())),
        }
    }

    /// Spawn a background task that periodically syncs position from the server.
    ///
    /// # Arguments
    ///
    /// * `nord` - Shared Nord client for REST queries.
    /// * `account_id` - Exchange account to query.
    /// * `market_id` - Market whose position to track.
    /// * `cancel` - Token to stop the sync loop.
    pub fn start_sync(
        &self,
        nord: Arc<nord::Nord>,
        account_id: u32,
        market_id: u32,
        cancel: CancellationToken,
    ) {
        let base_size = Arc::clone(&self.base_size);
        let interval_ms = self.config.sync_interval_ms;

        tokio::spawn(async move {
            // Sync once immediately before entering the loop.
            sync_from_server(&nord, account_id, market_id, &base_size).await;

            let mut interval = time::interval(Duration::from_millis(interval_ms));
            interval.tick().await; // consume immediate tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        sync_from_server(&nord, account_id, market_id, &base_size).await;
                    }
                    _ = cancel.cancelled() => {
                        debug!("position sync stopped");
                        return;
                    }
                }
            }
        });
    }

    /// Optimistically update position after a fill.
    ///
    /// Bid fills increase position (buying base), ask fills decrease it.
    pub fn apply_fill(&self, side: Side, size: f64) {
        loop {
            let old_bits = self.base_size.load(Ordering::Relaxed);
            let old = f64::from_bits(old_bits);
            let new = match side {
                Side::Bid => old + size,
                Side::Ask => old - size,
            };
            // CAS loop to handle concurrent fills.
            if self
                .base_size
                .compare_exchange_weak(
                    old_bits,
                    new.to_bits(),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                debug!(side = ?side, size, new_pos = new, "position updated from fill");
                return;
            }
        }
    }

    /// Build a [`QuotingContext`] from the current position and a fair price.
    pub fn get_quoting_context(&self, fair_price: f64) -> QuotingContext {
        let state = self.get_state(fair_price);
        let allowed_sides = allowed_sides(&state);
        QuotingContext {
            fair_price,
            position_state: state,
            allowed_sides,
        }
    }

    /// Current signed base-asset position.
    pub fn get_base_size(&self) -> f64 {
        f64::from_bits(self.base_size.load(Ordering::Relaxed))
    }

    /// Whether the current position triggers close mode at the given price.
    pub fn is_close_mode(&self, fair_price: f64) -> bool {
        let usd = (self.get_base_size() * fair_price).abs();
        usd >= self.config.close_threshold_usd
    }

    /// Build a [`PositionState`] from current base size and a fair price.
    fn get_state(&self, fair_price: f64) -> PositionState {
        let size_base = self.get_base_size();
        let size_usd = size_base * fair_price;
        let is_long = size_base > 0.0;
        let is_close_mode = size_usd.abs() >= self.config.close_threshold_usd;
        PositionState {
            size_base,
            size_usd,
            is_long,
            is_close_mode,
        }
    }
}

/// Determine which sides the quoter may trade given the position state.
fn allowed_sides(state: &PositionState) -> Vec<Side> {
    if state.is_close_mode {
        if state.is_long {
            vec![Side::Ask]
        } else {
            vec![Side::Bid]
        }
    } else {
        vec![Side::Bid, Side::Ask]
    }
}

/// Fetch the authoritative position from the server and correct drift.
async fn sync_from_server(
    nord: &nord::Nord,
    account_id: u32,
    market_id: u32,
    base_size: &AtomicU64,
) {
    let account = match nord.get_account(account_id).await {
        Ok(a) => a,
        Err(e) => {
            error!(error = %e, "position sync failed");
            return;
        }
    };

    let server_size = account
        .positions
        .iter()
        .find(|p| p.market_id == market_id)
        .and_then(|p| p.perp.as_ref())
        .map(|perp| {
            if perp.is_long {
                perp.base_size
            } else {
                -perp.base_size
            }
        })
        .unwrap_or(0.0);

    let local_size = f64::from_bits(base_size.load(Ordering::Relaxed));
    if (local_size - server_size).abs() > 0.0001 {
        warn!(
            local = format!("{local_size:.6}"),
            server = format!("{server_size:.6}"),
            "position drift detected — correcting"
        );
        base_size.store(server_size.to_bits(), Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(threshold: f64) -> PositionTracker {
        PositionTracker::new(PositionConfig {
            close_threshold_usd: threshold,
            sync_interval_ms: 5000,
        })
    }

    #[test]
    fn test_apply_fill_bid_increases_position() {
        let t = tracker(100.0);
        t.apply_fill(Side::Bid, 1.5);
        assert!((t.get_base_size() - 1.5).abs() < 1e-12);
    }

    #[test]
    fn test_apply_fill_ask_decreases_position() {
        let t = tracker(100.0);
        t.apply_fill(Side::Ask, 0.5);
        assert!((t.get_base_size() - (-0.5)).abs() < 1e-12);
    }

    #[test]
    fn test_close_mode_when_position_exceeds_threshold() {
        let t = tracker(10.0); // $10 threshold
        t.apply_fill(Side::Bid, 1.0);
        // 1.0 * $50 = $50 > $10 → close mode
        assert!(t.is_close_mode(50.0));
        // 1.0 * $5 = $5 < $10 → normal mode
        assert!(!t.is_close_mode(5.0));
    }

    #[test]
    fn test_normal_mode_allows_both_sides() {
        let t = tracker(1000.0);
        let ctx = t.get_quoting_context(100.0);
        assert!(!ctx.position_state.is_close_mode);
        assert_eq!(ctx.allowed_sides, vec![Side::Bid, Side::Ask]);
    }

    #[test]
    fn test_close_mode_long_only_allows_ask() {
        let t = tracker(10.0);
        t.apply_fill(Side::Bid, 1.0); // long 1.0
        let ctx = t.get_quoting_context(100.0); // $100 > $10 threshold
        assert!(ctx.position_state.is_close_mode);
        assert!(ctx.position_state.is_long);
        assert_eq!(ctx.allowed_sides, vec![Side::Ask]);
    }

    #[test]
    fn test_close_mode_short_only_allows_bid() {
        let t = tracker(10.0);
        t.apply_fill(Side::Ask, 1.0); // short -1.0
        let ctx = t.get_quoting_context(100.0); // $100 > $10 threshold
        assert!(ctx.position_state.is_close_mode);
        assert!(!ctx.position_state.is_long);
        assert_eq!(ctx.allowed_sides, vec![Side::Bid]);
    }

    #[test]
    fn test_quoting_context_computation() {
        let t = tracker(100.0);
        t.apply_fill(Side::Bid, 2.0);
        t.apply_fill(Side::Ask, 0.5);
        // net position = 1.5
        let ctx = t.get_quoting_context(50.0);
        assert!((ctx.position_state.size_base - 1.5).abs() < 1e-12);
        assert!((ctx.position_state.size_usd - 75.0).abs() < 1e-12);
        assert!(ctx.position_state.is_long);
        assert!(!ctx.position_state.is_close_mode); // $75 < $100
        assert_eq!(ctx.fair_price, 50.0);
    }

    #[test]
    fn test_is_close_mode_uses_fair_price_for_usd_calc() {
        let t = tracker(50.0);
        t.apply_fill(Side::Bid, 0.1);
        // 0.1 * $100 = $10 < $50 → not close
        assert!(!t.is_close_mode(100.0));
        // 0.1 * $600 = $60 >= $50 → close
        assert!(t.is_close_mode(600.0));
    }
}
