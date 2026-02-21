mod cli;
mod error;
mod feed;
mod output;
mod types;

use clap::Parser;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    let args = cli::Args::parse();

    // Initialize tracing
    let filter = args
        .log_level
        .parse::<tracing_subscriber::filter::LevelFilter>()
        .unwrap_or(tracing_subscriber::filter::LevelFilter::INFO);

    tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    info!(symbols = ?args.symbols, json = args.json, "binance-feed starting");

    let cancel = CancellationToken::new();

    // Signal handler
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("received SIGINT, shutting down");
        cancel_clone.cancel();
    });

    // Also handle SIGTERM on unix
    #[cfg(unix)]
    {
        let cancel_clone = cancel.clone();
        tokio::spawn(async move {
            let mut sig =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("failed to register SIGTERM handler");
            sig.recv().await;
            info!("received SIGTERM, shutting down");
            cancel_clone.cancel();
        });
    }

    feed::run_feed(&args.symbols, args.json, cancel).await;
}
