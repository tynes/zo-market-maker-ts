use clap::Parser;

/// Binance Futures book ticker feed — streams best bid/ask/mid prices.
#[derive(Parser, Debug)]
#[command(name = "binance-feed", version)]
pub struct Args {
    /// Trading pair symbols (e.g. btcusdt ethusdt solusdt)
    #[arg(required = true)]
    pub symbols: Vec<String>,

    /// Output as JSON instead of TSV
    #[arg(long)]
    pub json: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    pub log_level: String,
}
