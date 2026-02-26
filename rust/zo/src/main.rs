mod cli;
mod client;
mod error;
mod fair_price;
mod feed;
mod mm;
mod monitor;
mod orders;
mod output;
mod types;

use clap::Parser;
use cli::Command;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    let cli = cli::Cli::parse();

    // Initialize tracing
    let filter = cli
        .log_level
        .parse::<tracing_subscriber::filter::LevelFilter>()
        .unwrap_or(tracing_subscriber::filter::LevelFilter::INFO);

    tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    // Shared cancellation token + signal handlers.
    let cancel = setup_signal_handlers();

    match cli.command {
        Command::Feed(args) => {
            info!(symbols = ?args.symbols, json = args.json, "feed starting");
            feed::run_feed(&args.symbols, args.json, cancel).await;
        }

        Command::MarketMaker(args) => {
            let _ = dotenvy::dotenv(); // load .env if present

            let private_key = match std::env::var("PRIVATE_KEY") {
                Ok(k) => k,
                Err(_) => {
                    tracing::error!("PRIVATE_KEY environment variable is required");
                    std::process::exit(1);
                }
            };

            let config = mm::config::MarketMakerConfig {
                symbol: args.symbol.to_uppercase(),
                spread_bps: args.spread_bps,
                take_profit_bps: args.take_profit_bps,
                order_size_usd: args.order_size_usd,
                close_threshold_usd: args.close_threshold_usd,
                warmup_seconds: args.warmup_seconds,
                update_throttle_ms: args.update_throttle_ms,
                order_sync_interval_ms: args.order_sync_interval_ms,
                fair_price_window_ms: args.fair_price_window_ms,
                position_sync_interval_ms: args.position_sync_interval_ms,
                ..Default::default()
            };

            let bot = mm::bot::MarketMaker::new(config, private_key);
            if let Err(e) = bot.run(cancel).await {
                tracing::error!(error = %e, "market maker fatal error");
                std::process::exit(1);
            }
        }

        Command::Monitor(args) => {
            let _ = dotenvy::dotenv();
            if let Err(e) = monitor::run_monitor(&args.symbol, cancel).await {
                tracing::error!(error = %e, "monitor error");
                std::process::exit(1);
            }
        }
    }
}

/// Register SIGINT and SIGTERM handlers that trigger the returned token.
fn setup_signal_handlers() -> CancellationToken {
    let cancel = CancellationToken::new();

    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("received SIGINT, shutting down");
        cancel_clone.cancel();
    });

    #[cfg(unix)]
    {
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            let mut sig = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");
            sig.recv().await;
            info!("received SIGTERM, shutting down");
            cancel_clone.cancel();
        });
    }

    cancel
}
