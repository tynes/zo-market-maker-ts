//! Market maker configuration.

/// All tuneable parameters for the market maker bot.
///
/// Use [`Default::default()`] for sensible defaults, then set `symbol` before
/// passing to the bot.
#[derive(Debug, Clone)]
pub struct MarketMakerConfig {
    /// Market symbol prefix (e.g. "BTC" matches "BTC-PERP").
    pub symbol: String,
    /// Spread from fair price in basis points.
    pub spread_bps: f64,
    /// Tighter spread used in close (position reduction) mode, in basis points.
    pub take_profit_bps: f64,
    /// Notional order size in USD.
    pub order_size_usd: f64,
    /// Position threshold (USD) that triggers close mode.
    pub close_threshold_usd: f64,
    /// Seconds of price samples required before quoting.
    pub warmup_seconds: usize,
    /// Minimum interval between quote updates in milliseconds.
    pub update_throttle_ms: u64,
    /// Interval for syncing open orders from the API in milliseconds.
    pub order_sync_interval_ms: u64,
    /// Interval for status log lines in milliseconds.
    pub status_interval_ms: u64,
    /// Time window for fair price offset samples in milliseconds.
    pub fair_price_window_ms: u64,
    /// Interval for position sync from the server in milliseconds.
    pub position_sync_interval_ms: u64,
}

impl Default for MarketMakerConfig {
    fn default() -> Self {
        Self {
            symbol: String::new(),
            spread_bps: 8.0,
            take_profit_bps: 0.1,
            order_size_usd: 3000.0,
            close_threshold_usd: 10.0,
            warmup_seconds: 10,
            update_throttle_ms: 100,
            order_sync_interval_ms: 3000,
            status_interval_ms: 1000,
            fair_price_window_ms: 5 * 60 * 1000, // 5 minutes
            position_sync_interval_ms: 5000,
        }
    }
}
