//! Market monitor TUI using ratatui + crossterm.
//!
//! Displays live pricing (Binance vs 01 Exchange), orderbook depth, recent
//! trades, and a scrollable log panel.

use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ordered_float::OrderedFloat;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::client::mainnet_config;
use crate::error::ZoError;
use crate::fair_price::{FairPriceCalculator, FairPriceConfig};
use crate::feed::BinancePriceFeed;
use crate::mm::bot::derive_binance_symbol;

/// Fair price sample window (5 minutes).
const FAIR_PRICE_WINDOW_MS: u64 = 5 * 60 * 1000;

/// Minimum samples before fair price is valid.
const FAIR_PRICE_MIN_SAMPLES: usize = 10;

/// Window for computing updates-per-second.
const STATS_WINDOW_MS: u64 = 60_000;

/// Number of orderbook levels per side to display.
const ORDERBOOK_DEPTH: usize = 10;

/// Maximum number of recent trades to retain.
const MAX_TRADES: usize = 100;

/// Target render interval (10 FPS).
const RENDER_INTERVAL: Duration = Duration::from_millis(100);

/// Maximum log lines retained.
const MAX_LOG_LINES: usize = 500;

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// A single trade for display.
#[derive(Clone)]
struct DisplayTrade {
    time_ms: u64,
    side: nord::Side,
    price: f64,
    size: f64,
}

/// Tracks update timestamps for computing rates.
struct RateTracker {
    timestamps: VecDeque<u64>,
}

impl RateTracker {
    fn new() -> Self {
        Self {
            timestamps: VecDeque::new(),
        }
    }

    /// Record an update at the given epoch-millisecond timestamp.
    fn record(&mut self, now_ms: u64) {
        self.timestamps.push_back(now_ms);
        let cutoff = now_ms.saturating_sub(STATS_WINDOW_MS);
        while self.timestamps.front().is_some_and(|&t| t < cutoff) {
            self.timestamps.pop_front();
        }
    }

    /// Updates per second over the stats window.
    fn per_second(&self, now_ms: u64) -> f64 {
        let cutoff = now_ms.saturating_sub(STATS_WINDOW_MS);
        let count = self.timestamps.iter().filter(|&&t| t > cutoff).count();
        let first = self.timestamps.front().copied().unwrap_or(now_ms);
        let window_s = (now_ms.saturating_sub(first).min(STATS_WINDOW_MS) as f64) / 1000.0;
        if window_s <= 0.0 {
            0.0
        } else {
            count as f64 / window_s
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the market monitor TUI.
///
/// Connects to 01 Exchange (via Nord SDK) and Binance to display real-time
/// pricing, orderbook depth, trades, and fair price data.
///
/// # Arguments
///
/// * `symbol` - Market symbol prefix (e.g. "BTC", "ETH", "SOL").
/// * `cancel` - Cancellation token for graceful shutdown.
///
/// # Errors
///
/// Returns [`ZoError`] on connection or market-lookup failures.
pub async fn run_monitor(symbol: &str, cancel: CancellationToken) -> Result<(), ZoError> {
    let config = mainnet_config();
    let nord = Arc::new(nord::Nord::new(config).await?);

    // Find the matching market.
    let market = nord
        .markets
        .iter()
        .find(|m| m.symbol.to_uppercase().starts_with(&symbol.to_uppercase()))
        .ok_or_else(|| ZoError::MarketNotFound(symbol.to_string()))?;

    let market_symbol = market.symbol.clone();
    let price_decimals = market.price_decimals as usize;
    let size_decimals = market.size_decimals as usize;
    let binance_symbol = derive_binance_symbol(&market_symbol);

    info!(
        market = %market_symbol,
        binance = %binance_symbol,
        "starting monitor"
    );

    // Fair price calculator.
    let mut fair_calc = FairPriceCalculator::new(FairPriceConfig {
        window_ms: FAIR_PRICE_WINDOW_MS,
        min_samples: FAIR_PRICE_MIN_SAMPLES,
    });

    // Binance price feed.
    let binance_feed = BinancePriceFeed::new(&binance_symbol);
    binance_feed.connect();
    let mut binance_rx = binance_feed.subscribe_price();

    // 01 Exchange WebSocket (deltas + trades).
    let mut ws_client = nord.create_websocket_client(
        std::slice::from_ref(&market_symbol),
        std::slice::from_ref(&market_symbol),
        &[],
        &[],
    );
    ws_client.connect();

    // Orderbook stream.
    let delta_rx = ws_client.subscribe_deltas();
    let mut orderbook =
        nord::OrderbookStream::new(market_symbol.clone(), (*nord).clone(), delta_rx);
    orderbook.connect().await?;
    let mut ob_depth_rx = orderbook.subscribe_depth();
    let mut ob_price_rx = orderbook.subscribe_price();

    // Trade stream.
    let mut trade_rx: broadcast::Receiver<nord::WebSocketTradeUpdate> =
        ws_client.subscribe_trades();

    // Mutable display state.
    let mut binance_price: Option<nord::MidPrice> = None;
    let mut zo_price: Option<nord::MidPrice> = None;
    let mut fair_price_value: Option<f64> = None;
    let mut ob_bids: BTreeMap<OrderedFloat<f64>, f64> = BTreeMap::new();
    let mut ob_asks: BTreeMap<OrderedFloat<f64>, f64> = BTreeMap::new();
    let mut recent_trades: VecDeque<DisplayTrade> = VecDeque::with_capacity(MAX_TRADES);
    let mut log_lines: VecDeque<String> = VecDeque::with_capacity(MAX_LOG_LINES);

    let mut binance_rate = RateTracker::new();
    let mut zo_rate = RateTracker::new();

    log_lines.push_back(format!(
        "Market: {market_symbol}, Binance: {binance_symbol}"
    ));
    log_lines.push_back("Connecting...".to_string());

    // Set up terminal.
    enable_raw_mode().map_err(|_| ZoError::Config("failed to enable raw mode".into()))?;
    io::stdout()
        .execute(EnterAlternateScreen)
        .map_err(|_| ZoError::Config("failed to enter alternate screen".into()))?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))
        .map_err(|_| ZoError::Config("failed to create terminal".into()))?;

    log_lines.push_back("Connected! Press 'q' to quit.".to_string());

    let mut render_interval = tokio::time::interval(RENDER_INTERVAL);

    // Main event loop.
    let mut quit = false;
    let result: Result<(), ZoError> = loop {
        if quit {
            break Ok(());
        }

        tokio::select! {
            // Binance price update.
            Ok(()) = binance_rx.changed() => {
                let now = epoch_ms();
                if let Some(p) = *binance_rx.borrow_and_update() {
                    binance_price = Some(p);
                    binance_rate.record(now);
                    update_fair_price(
                        &mut fair_calc,
                        &mut fair_price_value,
                        binance_price.as_ref(),
                        zo_price.as_ref(),
                        now,
                    );
                }
            }

            // 01 Exchange price update.
            Ok(()) = ob_price_rx.changed() => {
                let now = epoch_ms();
                if let Some(p) = *ob_price_rx.borrow_and_update() {
                    zo_price = Some(p);
                    zo_rate.record(now);
                    update_fair_price(
                        &mut fair_calc,
                        &mut fair_price_value,
                        binance_price.as_ref(),
                        zo_price.as_ref(),
                        now,
                    );
                }
            }

            // Orderbook depth update.
            Ok(()) = ob_depth_rx.changed() => {
                if let Some(depth) = ob_depth_rx.borrow_and_update().clone() {
                    ob_bids = depth.bids;
                    ob_asks = depth.asks;
                }
            }

            // Trade update.
            result = trade_rx.recv() => {
                match result {
                    Ok(update) => {
                        if update.market_symbol == market_symbol {
                            let now = epoch_ms();
                            for t in &update.trades {
                                recent_trades.push_front(DisplayTrade {
                                    time_ms: now,
                                    side: t.side,
                                    price: t.price,
                                    size: t.size,
                                });
                            }
                            while recent_trades.len() > MAX_TRADES {
                                recent_trades.pop_back();
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log_lines.push_back(format!("trade stream lagged by {n}"));
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        log_lines.push_back("trade stream closed".to_string());
                    }
                }
            }

            // Render tick — also polls keyboard input.
            _ = render_interval.tick() => {
                // Poll crossterm events (non-blocking).
                while event::poll(Duration::ZERO).unwrap_or(false) {
                    if let Ok(Event::Key(key)) = event::read() {
                        if key.kind == KeyEventKind::Press
                            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                        {
                            quit = true;
                        }
                    }
                }

                if !quit {
                    let now = epoch_ms();
                    let _ = terminal.draw(|frame| {
                        render_ui(
                            frame,
                            symbol,
                            binance_price.as_ref(),
                            zo_price.as_ref(),
                            fair_price_value,
                            &fair_calc,
                            &binance_rate,
                            &zo_rate,
                            &ob_bids,
                            &ob_asks,
                            &recent_trades,
                            &log_lines,
                            price_decimals,
                            size_decimals,
                            now,
                        );
                    });
                }
            }

            _ = cancel.cancelled() => {
                break Ok(());
            }
        }
    };

    // Restore terminal.
    restore_terminal(&mut terminal);

    // Clean up.
    binance_feed.close();
    orderbook.close();

    result
}

// ---------------------------------------------------------------------------
// Fair price update helper
// ---------------------------------------------------------------------------

/// Update the fair price calculator and cached value.
fn update_fair_price(
    calc: &mut FairPriceCalculator,
    cached: &mut Option<f64>,
    binance: Option<&nord::MidPrice>,
    zo: Option<&nord::MidPrice>,
    now_ms: u64,
) {
    if let (Some(b), Some(z)) = (binance, zo) {
        // Only add sample if both prices are within 1 second.
        if b.timestamp.abs_diff(z.timestamp) < 1000 {
            calc.add_sample(z.mid, b.mid, now_ms);
        }
        *cached = calc.get_fair_price(b.mid, now_ms);
    }
}

// ---------------------------------------------------------------------------
// Terminal helpers
// ---------------------------------------------------------------------------

/// Restore terminal to normal mode.
fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) {
    let _ = terminal.show_cursor();
    let _ = disable_raw_mode();
    let _ = io::stdout().execute(LeaveAlternateScreen);
}

/// Current epoch milliseconds.
fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// UI rendering
// ---------------------------------------------------------------------------

/// Render the full TUI frame.
#[allow(clippy::too_many_arguments)]
fn render_ui(
    frame: &mut Frame,
    symbol: &str,
    binance_price: Option<&nord::MidPrice>,
    zo_price: Option<&nord::MidPrice>,
    fair_price: Option<f64>,
    fair_calc: &FairPriceCalculator,
    binance_rate: &RateTracker,
    zo_rate: &RateTracker,
    ob_bids: &BTreeMap<OrderedFloat<f64>, f64>,
    ob_asks: &BTreeMap<OrderedFloat<f64>, f64>,
    recent_trades: &VecDeque<DisplayTrade>,
    log_lines: &VecDeque<String>,
    price_decimals: usize,
    size_decimals: usize,
    now_ms: u64,
) {
    let area = frame.area();

    // Layout: header (3 rows), top panels (60%), bottom log (rest).
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // header
            Constraint::Ratio(3, 5), // top panels
            Constraint::Min(5),      // log
        ])
        .split(area);

    // Header.
    let header_text = format!(
        " ZO MARKET MONITOR - {} | 10 FPS | 'q' to quit",
        symbol.to_uppercase(),
    );
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::White).bg(Color::Blue).bold())
        .alignment(Alignment::Center);
    frame.render_widget(header, main_layout[0]);

    // Top panels: pricing (20%) | orderbook (30%) | trades (50%).
    let top_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(30),
            Constraint::Percentage(50),
        ])
        .split(main_layout[1]);

    // Render each panel.
    render_pricing(
        frame,
        top_layout[0],
        binance_price,
        zo_price,
        fair_price,
        fair_calc,
        binance_rate,
        zo_rate,
        price_decimals,
        now_ms,
    );
    render_orderbook(
        frame,
        top_layout[1],
        ob_bids,
        ob_asks,
        price_decimals,
        size_decimals,
    );
    render_trades(
        frame,
        top_layout[2],
        recent_trades,
        price_decimals,
        size_decimals,
    );
    render_log(frame, main_layout[2], log_lines);
}

/// Render the pricing panel.
#[allow(clippy::too_many_arguments)]
fn render_pricing(
    frame: &mut Frame,
    area: Rect,
    binance_price: Option<&nord::MidPrice>,
    zo_price: Option<&nord::MidPrice>,
    fair_price: Option<f64>,
    fair_calc: &FairPriceCalculator,
    binance_rate: &RateTracker,
    zo_rate: &RateTracker,
    price_decimals: usize,
    now_ms: u64,
) {
    let mut lines = Vec::with_capacity(8);

    // Binance line.
    if let Some(p) = binance_price {
        let rate = binance_rate.per_second(now_ms);
        lines.push(Line::from(vec![
            Span::raw(format!(
                " Binance ${:.prec$} ",
                p.mid,
                prec = price_decimals,
            )),
            Span::styled(format!("{rate:.1}/s"), Style::default().fg(Color::DarkGray)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::raw(" Binance "),
            Span::styled("--", Style::default().fg(Color::Yellow)),
        ]));
    }

    // 01 Exchange line.
    if let Some(p) = zo_price {
        let rate = zo_rate.per_second(now_ms);
        lines.push(Line::from(vec![
            Span::raw(format!(
                " 01      ${:.prec$} ",
                p.mid,
                prec = price_decimals,
            )),
            Span::styled(format!("{rate:.1}/s"), Style::default().fg(Color::DarkGray)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::raw(" 01      "),
            Span::styled("--", Style::default().fg(Color::Yellow)),
        ]));
    }

    // Current offset.
    if let (Some(b), Some(z)) = (binance_price, zo_price) {
        let offset = z.mid - b.mid;
        let offset_bps = (offset / b.mid) * 10_000.0;
        let sign = if offset >= 0.0 { "+" } else { "" };
        lines.push(Line::from(format!(" Offset  {sign}{offset_bps:.1}bps")));
    }

    // Median offset.
    let state = fair_calc.get_state(now_ms);
    if let (Some(offset), Some(b)) = (state.offset, binance_price) {
        let median_bps = (offset / b.mid) * 10_000.0;
        let sign = if offset >= 0.0 { "+" } else { "" };
        lines.push(Line::from(vec![
            Span::raw(format!(" Median  {sign}{median_bps:.1}bps ")),
            Span::styled(
                format!("({}s)", state.samples),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    // Fair price.
    if let (Some(fp), Some(_b)) = (fair_price, binance_price) {
        lines.push(Line::from(format!(
            " Fair    ${fp:.prec$}",
            prec = price_decimals,
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Pricing ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Render the orderbook panel.
fn render_orderbook(
    frame: &mut Frame,
    area: Rect,
    bids: &BTreeMap<OrderedFloat<f64>, f64>,
    asks: &BTreeMap<OrderedFloat<f64>, f64>,
    price_decimals: usize,
    size_decimals: usize,
) {
    let mut lines: Vec<Line> = Vec::with_capacity(ORDERBOOK_DEPTH * 2 + 4);

    lines.push(Line::from(""));
    lines.push(Line::from(
        "       Price       Size         USD".to_string(),
    ));
    lines.push(Line::from(
        "  ──────────────────────────────────────".to_string(),
    ));

    // Asks: take the closest N to the spread (lowest prices), display reversed.
    let sorted_asks: Vec<(f64, f64)> = asks
        .iter()
        .take(ORDERBOOK_DEPTH)
        .map(|(p, s)| (p.0, *s))
        .collect();

    // Pad empty lines if fewer than ORDERBOOK_DEPTH asks.
    for _ in sorted_asks.len()..ORDERBOOK_DEPTH {
        lines.push(Line::from(""));
    }
    // Display asks in reverse (highest at top, lowest near spread).
    for &(price, size) in sorted_asks.iter().rev() {
        let usd = price * size;
        let line_str = format!(
            "  {price:>width_p$.prec_p$} {size:>width_s$.prec_s$}{usd:>12.0}",
            width_p = 11,
            prec_p = price_decimals,
            width_s = 10,
            prec_s = size_decimals,
        );
        lines.push(Line::styled(line_str, Style::default().fg(Color::Red)));
    }

    // Spread line.
    let best_bid = bids.keys().next_back().map(|k| k.0).unwrap_or(0.0);
    let best_ask = asks.keys().next().map(|k| k.0).unwrap_or(0.0);
    let spread = best_ask - best_bid;
    let spread_bps = if best_bid > 0.0 {
        (spread / best_bid) * 10_000.0
    } else {
        0.0
    };
    lines.push(Line::from(format!(
        "  --- spread: {spread:.prec$} ({spread_bps:.1} bps) ---",
        prec = price_decimals,
    )));

    // Bids: highest first.
    let sorted_bids: Vec<(f64, f64)> = bids
        .iter()
        .rev()
        .take(ORDERBOOK_DEPTH)
        .map(|(p, s)| (p.0, *s))
        .collect();

    for &(price, size) in &sorted_bids {
        let usd = price * size;
        let line_str = format!(
            "  {price:>width_p$.prec_p$} {size:>width_s$.prec_s$}{usd:>12.0}",
            width_p = 11,
            prec_p = price_decimals,
            width_s = 10,
            prec_s = size_decimals,
        );
        lines.push(Line::styled(line_str, Style::default().fg(Color::Green)));
    }
    // Pad empty lines if fewer than ORDERBOOK_DEPTH bids.
    for _ in sorted_bids.len()..ORDERBOOK_DEPTH {
        lines.push(Line::from(""));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Orderbook ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Render the trades panel.
fn render_trades(
    frame: &mut Frame,
    area: Rect,
    recent_trades: &VecDeque<DisplayTrade>,
    price_decimals: usize,
    size_decimals: usize,
) {
    let lines: Vec<Line> = recent_trades
        .iter()
        .map(|t| format_trade(t, price_decimals, size_decimals))
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Trades ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Format a single trade line with timestamp, price, size, and USD value.
fn format_trade(trade: &DisplayTrade, price_decimals: usize, size_decimals: usize) -> Line<'_> {
    // Format time as HH:MM:SS.mmm.
    let secs = trade.time_ms / 1000;
    let millis = trade.time_ms % 1000;
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    let time_str = format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}");

    let is_buy = trade.side == nord::Side::Ask; // taker hit the ask = buy
    let sign = if is_buy { "" } else { "-" };
    let color = if is_buy { Color::Green } else { Color::Red };
    let usd = trade.price * trade.size;

    let price_str = format!(
        "{:>width$.prec$}",
        trade.price,
        width = 10,
        prec = price_decimals
    );
    let size_str = format!("{sign}{:.prec$}", trade.size, prec = size_decimals,);
    let usd_str = format!("{sign}{usd:.0}");

    Line::from(vec![
        Span::raw(format!("{time_str}  {price_str}  ")),
        Span::styled(
            format!("{size_str:>9}  {usd_str:>11}"),
            Style::default().fg(color),
        ),
    ])
}

/// Render the log panel.
fn render_log(frame: &mut Frame, area: Rect, log_lines: &VecDeque<String>) {
    let lines: Vec<Line> = log_lines.iter().map(|l| Line::from(l.as_str())).collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Log ");
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
