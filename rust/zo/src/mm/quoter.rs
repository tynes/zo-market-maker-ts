//! Quote generator — computes bid/ask prices with proper tick/lot alignment.
//!
//! Uses `rust_decimal::Decimal` for deterministic fixed-point arithmetic
//! so that prices and sizes align exactly to exchange tick and lot sizes.

use nord::{Side, BBO};
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::mm::position::QuotingContext;
use crate::types::Quote;

/// Generates bid/ask quotes from a fair price, position state, and BBO.
pub struct Quoter {
    /// Minimum price increment (`10^-price_decimals`).
    tick_size: Decimal,
    /// Minimum size increment (`10^-size_decimals`).
    lot_size: Decimal,
    /// Normal-mode spread in basis points.
    spread_bps: Decimal,
    /// Close-mode spread in basis points.
    take_profit_bps: Decimal,
    /// Notional order size in USD (normal mode).
    order_size_usd: Decimal,
}

/// Rounding direction for tick alignment.
enum RoundMode {
    Floor,
    Ceil,
}

impl Quoter {
    /// Create a new quoter from market parameters.
    ///
    /// # Arguments
    ///
    /// * `price_decimals` - Number of decimal places for prices.
    /// * `size_decimals` - Number of decimal places for sizes.
    /// * `spread_bps` - Normal-mode spread in basis points.
    /// * `take_profit_bps` - Close-mode spread in basis points.
    /// * `order_size_usd` - Notional USD size per quote.
    pub fn new(
        price_decimals: u8,
        size_decimals: u8,
        spread_bps: f64,
        take_profit_bps: f64,
        order_size_usd: f64,
    ) -> Self {
        Self {
            tick_size: Decimal::new(1, price_decimals as u32),
            lot_size: Decimal::new(1, size_decimals as u32),
            spread_bps: Decimal::from_f64(spread_bps).unwrap_or(dec!(8)),
            take_profit_bps: Decimal::from_f64(take_profit_bps).unwrap_or(dec!(0.1)),
            order_size_usd: Decimal::from_f64(order_size_usd).unwrap_or(dec!(3000)),
        }
    }

    /// Generate quotes for the given quoting context, clamped to the BBO.
    ///
    /// In normal mode: both bid and ask at `spread_bps` from fair.
    /// In close mode: only the reducing side at `take_profit_bps`.
    pub fn get_quotes(&self, ctx: &QuotingContext, bbo: Option<&BBO>) -> Vec<Quote> {
        let fair = Decimal::from_f64(ctx.fair_price).unwrap_or_default();
        let bps = if ctx.position_state.is_close_mode {
            self.take_profit_bps
        } else {
            self.spread_bps
        };
        let spread_amount = fair * bps / dec!(10000);

        // In close mode: limit size to position size.
        let size = if ctx.position_state.is_close_mode {
            let pos_size =
                Decimal::from_f64(ctx.position_state.size_base.abs()).unwrap_or_default();
            self.align_size(pos_size)
        } else {
            self.usd_to_size(fair)
        };

        if size <= Decimal::ZERO {
            return Vec::new();
        }

        let mut quotes = Vec::with_capacity(2);

        if ctx.allowed_sides.contains(&Side::Bid) {
            let mut bid_price = self.align_price(fair - spread_amount, RoundMode::Floor);

            // Clamp bid below best ask (don't cross spread).
            if let Some(bbo) = bbo {
                let best_ask = Decimal::from_f64(bbo.best_ask).unwrap_or_default();
                if bid_price >= best_ask {
                    bid_price = self.align_price(best_ask - self.tick_size, RoundMode::Floor);
                }
            }

            if bid_price > Decimal::ZERO {
                quotes.push(Quote {
                    side: Side::Bid,
                    price: bid_price,
                    size,
                });
            }
        }

        if ctx.allowed_sides.contains(&Side::Ask) {
            let mut ask_price = self.align_price(fair + spread_amount, RoundMode::Ceil);

            // Clamp ask above best bid (don't cross spread).
            if let Some(bbo) = bbo {
                let best_bid = Decimal::from_f64(bbo.best_bid).unwrap_or_default();
                if ask_price <= best_bid {
                    ask_price = self.align_price(best_bid + self.tick_size, RoundMode::Ceil);
                }
            }

            if ask_price > Decimal::ZERO {
                quotes.push(Quote {
                    side: Side::Ask,
                    price: ask_price,
                    size,
                });
            }
        }

        quotes
    }

    /// Align a price to the tick size.
    fn align_price(&self, price: Decimal, mode: RoundMode) -> Decimal {
        let ticks = price / self.tick_size;
        let aligned = match mode {
            RoundMode::Floor => ticks.floor(),
            RoundMode::Ceil => ticks.ceil(),
        };
        aligned * self.tick_size
    }

    /// Convert a USD notional to base-asset size, aligned to lot size.
    fn usd_to_size(&self, fair_price: Decimal) -> Decimal {
        if fair_price.is_zero() {
            return Decimal::ZERO;
        }
        let raw = self.order_size_usd / fair_price;
        self.align_size(raw)
    }

    /// Align a size to the lot size (always rounds down).
    fn align_size(&self, size: Decimal) -> Decimal {
        let lots = (size / self.lot_size).floor();
        lots * self.lot_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm::position::PositionState;

    /// Helper to build a normal-mode context.
    fn normal_ctx(fair: f64) -> QuotingContext {
        QuotingContext {
            fair_price: fair,
            position_state: PositionState {
                size_base: 0.0,
                size_usd: 0.0,
                is_long: false,
                is_close_mode: false,
            },
            allowed_sides: vec![Side::Bid, Side::Ask],
        }
    }

    /// Helper to build a close-mode context.
    fn close_ctx(fair: f64, size_base: f64, allowed: Vec<Side>) -> QuotingContext {
        let is_long = size_base > 0.0;
        QuotingContext {
            fair_price: fair,
            position_state: PositionState {
                size_base,
                size_usd: size_base * fair,
                is_long,
                is_close_mode: true,
            },
            allowed_sides: allowed,
        }
    }

    fn quoter() -> Quoter {
        // price_decimals=2, size_decimals=4, spread=8bps, tp=0.1bps, size=$3000
        Quoter::new(2, 4, 8.0, 0.1, 3000.0)
    }

    #[test]
    fn test_normal_mode_generates_bid_and_ask() {
        let q = quoter();
        let quotes = q.get_quotes(&normal_ctx(50000.0), None);
        assert_eq!(quotes.len(), 2);
        assert_eq!(quotes[0].side, Side::Bid);
        assert_eq!(quotes[1].side, Side::Ask);
    }

    #[test]
    fn test_spread_applied_correctly() {
        let q = quoter();
        let fair = 50000.0;
        let quotes = q.get_quotes(&normal_ctx(fair), None);
        let bid = quotes[0].price;
        let ask = quotes[1].price;
        // 8 bps of $50000 = $40
        // bid should be <= fair - 40, ask should be >= fair + 40
        let fair_d = Decimal::from_f64(fair).unwrap();
        assert!(bid <= fair_d - dec!(40));
        assert!(ask >= fair_d + dec!(40));
    }

    #[test]
    fn test_close_mode_uses_tighter_spread() {
        let q = quoter();
        let fair = 50000.0;
        // Close mode long → only ask
        let ctx = close_ctx(fair, 0.06, vec![Side::Ask]);
        let quotes = q.get_quotes(&ctx, None);
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].side, Side::Ask);
        let ask = quotes[0].price;
        // 0.1 bps of 50000 = 50000 * 0.1 / 10000 = 0.50
        let fair_d = Decimal::from_f64(fair).unwrap();
        assert!(ask >= fair_d + dec!(0.50));
        // But much less than 8 bps ($40)
        assert!(ask < fair_d + dec!(40));
    }

    #[test]
    fn test_close_mode_size_limited_to_position() {
        let q = quoter();
        let ctx = close_ctx(50000.0, 0.1234, vec![Side::Ask]);
        let quotes = q.get_quotes(&ctx, None);
        assert_eq!(quotes.len(), 1);
        // Size should be align_size(0.1234) with lot_size=0.0001 → 0.1234
        assert_eq!(quotes[0].size, dec!(0.1234));
    }

    #[test]
    fn test_bid_clamped_below_best_ask() {
        let q = quoter();
        let bbo = BBO {
            best_bid: 49950.0,
            best_ask: 49960.0, // very tight spread; fair - 8bps might cross
        };
        let fair = 49960.0; // fair = best_ask exactly
        let ctx = normal_ctx(fair);
        let quotes = q.get_quotes(&ctx, Some(&bbo));
        let bid = quotes.iter().find(|q| q.side == Side::Bid).unwrap();
        // Bid must be strictly below best_ask
        assert!(bid.price < Decimal::from_f64(bbo.best_ask).unwrap());
    }

    #[test]
    fn test_ask_clamped_above_best_bid() {
        let q = quoter();
        let bbo = BBO {
            best_bid: 50040.0, // very high bid
            best_ask: 50050.0,
        };
        let fair = 50040.0; // fair = best_bid
        let ctx = normal_ctx(fair);
        let quotes = q.get_quotes(&ctx, Some(&bbo));
        let ask = quotes.iter().find(|q| q.side == Side::Ask).unwrap();
        // Ask must be strictly above best_bid
        assert!(ask.price > Decimal::from_f64(bbo.best_bid).unwrap());
    }

    #[test]
    fn test_price_aligned_to_tick_size() {
        let q = quoter(); // tick_size = 0.01
        let quotes = q.get_quotes(&normal_ctx(50000.123), None);
        for quote in &quotes {
            // price / tick_size should be an integer (i.e. scale=0 after division)
            let ticks = quote.price / dec!(0.01);
            assert_eq!(ticks, ticks.floor());
        }
    }

    #[test]
    fn test_size_aligned_to_lot_size() {
        let q = quoter(); // lot_size = 0.0001
        let quotes = q.get_quotes(&normal_ctx(50000.0), None);
        for quote in &quotes {
            let lots = quote.size / dec!(0.0001);
            assert_eq!(lots, lots.floor());
        }
    }

    #[test]
    fn test_zero_size_returns_empty_quotes() {
        // Close mode with essentially zero position → size rounds to 0
        let q = quoter(); // lot_size = 0.0001
        let ctx = close_ctx(50000.0, 0.00001, vec![Side::Ask]); // < 1 lot
        let quotes = q.get_quotes(&ctx, None);
        assert!(quotes.is_empty());
    }

    #[test]
    fn test_usd_to_size_conversion() {
        let q = quoter(); // $3000 USD, lot_size 0.0001
                          // At $50000/unit: 3000/50000 = 0.06 → 600 lots → 0.0600
        let size = q.usd_to_size(dec!(50000));
        assert_eq!(size, dec!(0.0600));
    }
}
