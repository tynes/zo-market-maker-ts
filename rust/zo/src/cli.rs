use clap::{Parser, Subcommand};

/// zo — unified CLI for the zo market maker project.
#[derive(Parser, Debug)]
#[command(name = "zo", version)]
pub struct Cli {
    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", global = true)]
    pub log_level: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Stream Binance Futures best bid/ask/mid prices
    Feed(FeedArgs),

    /// Run the market maker bot
    MarketMaker(MarketMakerArgs),

    /// Launch the market monitor TUI
    Monitor(MonitorArgs),
}

/// Arguments for the `feed` subcommand.
#[derive(Parser, Debug)]
pub struct FeedArgs {
    /// Trading pair symbols (e.g. btcusdt ethusdt solusdt)
    #[arg(required = true)]
    pub symbols: Vec<String>,

    /// Output as JSON instead of TSV
    #[arg(long)]
    pub json: bool,
}

/// Arguments for the `market-maker` subcommand.
#[derive(Parser, Debug)]
pub struct MarketMakerArgs {
    /// Market symbol prefix (e.g. BTC, ETH, SOL)
    pub symbol: String,

    /// Spread from fair price in basis points
    #[arg(long, default_value = "8")]
    pub spread_bps: f64,

    /// Spread in close (position-reduction) mode in basis points
    #[arg(long, default_value = "0.1")]
    pub take_profit_bps: f64,

    /// Order size in USD
    #[arg(long, default_value = "3000")]
    pub order_size_usd: f64,

    /// Position USD threshold that triggers close mode
    #[arg(long, default_value = "10")]
    pub close_threshold_usd: f64,

    /// Seconds of price samples before quoting
    #[arg(long, default_value = "10")]
    pub warmup_seconds: usize,

    /// Minimum interval between quote updates (ms)
    #[arg(long, default_value = "100")]
    pub update_throttle_ms: u64,

    /// Interval for syncing orders from the API (ms)
    #[arg(long, default_value = "3000")]
    pub order_sync_interval_ms: u64,

    /// Fair price sample window (ms)
    #[arg(long, default_value = "300000")]
    pub fair_price_window_ms: u64,

    /// Interval for position sync from the server (ms)
    #[arg(long, default_value = "5000")]
    pub position_sync_interval_ms: u64,
}

/// Arguments for the `monitor` subcommand.
#[derive(Parser, Debug)]
pub struct MonitorArgs {
    /// Market symbol prefix (e.g. BTC, ETH, SOL)
    pub symbol: String,
}
