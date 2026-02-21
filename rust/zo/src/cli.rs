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

    /// Run the market maker (not yet implemented)
    MarketMaker,
}

#[derive(Parser, Debug)]
pub struct FeedArgs {
    /// Trading pair symbols (e.g. btcusdt ethusdt solusdt)
    #[arg(required = true)]
    pub symbols: Vec<String>,

    /// Output as JSON instead of TSV
    #[arg(long)]
    pub json: bool,
}
