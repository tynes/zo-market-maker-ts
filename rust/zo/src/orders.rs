//! Atomic order management — diff, cancel, and place in chunks of 4.
//!
//! Ports `src/sdk/orders.ts`. Compares current active orders against new
//! desired quotes, cancels stale orders and places new ones atomically.

use nord::actions::atomic::UserAtomicSubaction;
use nord::proto::nord as proto;
use nord::{FillMode, NordUser, Side};
use rust_decimal::Decimal;
use tracing::{debug, info};

use crate::error::ZoError;
use crate::types::Quote;

/// Maximum subactions per atomic call (exchange limit).
const MAX_ATOMIC_ACTIONS: usize = 4;

/// An order whose ID is known (kept from a previous atomic result or sync).
#[derive(Debug, Clone)]
pub struct CachedOrder {
    pub order_id: u64,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Diff `current_orders` against `new_quotes`, cancel stale orders, place new
/// ones, and return the resulting active-order set.
///
/// Orders that already match a quote (same side, price, size) are kept without
/// touching the exchange.
pub async fn update_quotes(
    user: &NordUser,
    market_id: u32,
    current_orders: &[CachedOrder],
    new_quotes: &[Quote],
) -> Result<Vec<CachedOrder>, ZoError> {
    let (kept, to_cancel, to_place) = diff_orders(current_orders, new_quotes);

    if to_cancel.is_empty() && to_place.is_empty() {
        return Ok(current_orders.to_vec());
    }

    // Build actions: cancels first, then places.
    let mut actions: Vec<UserAtomicSubaction> =
        Vec::with_capacity(to_cancel.len() + to_place.len());
    for order in &to_cancel {
        actions.push(build_cancel_action(order.order_id));
    }
    for quote in &to_place {
        actions.push(build_place_action(market_id, quote));
    }

    let placed = execute_atomic(user, &actions).await?;

    let mut result = kept;
    result.extend(placed);
    Ok(result)
}

/// Cancel all given orders atomically (in chunks of 4).
pub async fn cancel_orders(user: &NordUser, orders: &[CachedOrder]) -> Result<(), ZoError> {
    if orders.is_empty() {
        return Ok(());
    }
    let actions: Vec<UserAtomicSubaction> = orders
        .iter()
        .map(|o| build_cancel_action(o.order_id))
        .collect();
    execute_atomic(user, &actions).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Pure diffing logic (unit-testable)
// ---------------------------------------------------------------------------

/// Returns (kept_orders, orders_to_cancel, quotes_to_place).
fn diff_orders<'a>(
    current: &'a [CachedOrder],
    new_quotes: &'a [Quote],
) -> (Vec<CachedOrder>, Vec<&'a CachedOrder>, Vec<&'a Quote>) {
    let mut kept = Vec::new();
    let mut to_place = Vec::new();
    let mut matched_indices = vec![false; current.len()];

    for quote in new_quotes {
        let found = current
            .iter()
            .enumerate()
            .position(|(i, o)| !matched_indices[i] && order_matches_quote(o, quote));
        if let Some(idx) = found {
            matched_indices[idx] = true;
            kept.push(current[idx].clone());
        } else {
            to_place.push(quote);
        }
    }

    let to_cancel: Vec<&CachedOrder> = current
        .iter()
        .enumerate()
        .filter(|(i, _)| !matched_indices[*i])
        .map(|(_, o)| o)
        .collect();

    (kept, to_cancel, to_place)
}

/// Check if an existing order matches a desired quote (same side + price + size).
fn order_matches_quote(order: &CachedOrder, quote: &Quote) -> bool {
    order.side == quote.side && order.price == quote.price && order.size == quote.size
}

// ---------------------------------------------------------------------------
// Action builders
// ---------------------------------------------------------------------------

fn build_place_action(market_id: u32, quote: &Quote) -> UserAtomicSubaction {
    UserAtomicSubaction::Place {
        market_id,
        side: quote.side,
        fill_mode: FillMode::PostOnly,
        is_reduce_only: false,
        size: Some(quote.size),
        price: Some(quote.price),
        quote_size: None,
        client_order_id: None,
    }
}

fn build_cancel_action(order_id: u64) -> UserAtomicSubaction {
    UserAtomicSubaction::Cancel { order_id }
}

// ---------------------------------------------------------------------------
// Atomic execution (chunked)
// ---------------------------------------------------------------------------

/// Execute actions in chunks of [`MAX_ATOMIC_ACTIONS`], returning any placed orders.
async fn execute_atomic(
    user: &NordUser,
    actions: &[UserAtomicSubaction],
) -> Result<Vec<CachedOrder>, ZoError> {
    if actions.is_empty() {
        return Ok(Vec::new());
    }

    let mut all_placed = Vec::new();
    let total_chunks = actions.len().div_ceil(MAX_ATOMIC_ACTIONS);

    for (chunk_idx, chunk) in actions.chunks(MAX_ATOMIC_ACTIONS).enumerate() {
        info!(
            chunk = chunk_idx + 1,
            total = total_chunks,
            actions = format_actions(chunk),
            "ATOMIC"
        );

        let result = user.atomic(chunk, None).await.map_err(ZoError::Nord)?;
        let placed = extract_placed_orders(&result.results, chunk);

        if !placed.is_empty() {
            debug!(
                ids = ?placed.iter().map(|o| o.order_id).collect::<Vec<_>>(),
                "placed orders"
            );
        }
        all_placed.extend(placed);
    }

    Ok(all_placed)
}

/// Extract placed orders from atomic result, pairing each `PlaceOrderResult`
/// with its corresponding `Place` action to recover side/price/size.
fn extract_placed_orders(
    results: &[proto::receipt::atomic_subaction_result_kind::Inner],
    actions: &[UserAtomicSubaction],
) -> Vec<CachedOrder> {
    let place_actions: Vec<&UserAtomicSubaction> = actions
        .iter()
        .filter(|a| matches!(a, UserAtomicSubaction::Place { .. }))
        .collect();

    let mut orders = Vec::new();
    let mut place_idx = 0;

    for r in results {
        if let proto::receipt::atomic_subaction_result_kind::Inner::PlaceOrderResult(por) = r {
            if let Some(posted) = &por.posted {
                if let Some(UserAtomicSubaction::Place {
                    side, price, size, ..
                }) = place_actions.get(place_idx).copied()
                {
                    orders.push(CachedOrder {
                        order_id: posted.order_id,
                        side: *side,
                        price: price.unwrap_or_default(),
                        size: size.unwrap_or_default(),
                    });
                }
            }
            place_idx += 1;
        }
    }

    orders
}

/// Format actions for logging.
fn format_actions(actions: &[UserAtomicSubaction]) -> String {
    actions
        .iter()
        .map(|a| match a {
            UserAtomicSubaction::Cancel { order_id } => format!("X{order_id}"),
            UserAtomicSubaction::Place {
                side,
                price,
                size,
                fill_mode,
                is_reduce_only,
                ..
            } => {
                let s = match side {
                    Side::Bid => "B",
                    Side::Ask => "A",
                };
                let ro = if *is_reduce_only { "RO" } else { "" };
                let fm = match fill_mode {
                    FillMode::PostOnly => "PO",
                    FillMode::Limit => "LIM",
                    FillMode::ImmediateOrCancel => "IOC",
                    FillMode::FillOrKill => "FOK",
                };
                let p = price.map(|d| d.to_string()).unwrap_or_default();
                let sz = size.map(|d| d.to_string()).unwrap_or_default();
                format!("{s}{ro}[{fm}]@{p}x{sz}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Testable helpers for the pure diff logic
// ---------------------------------------------------------------------------

/// Simplified diff for testing: returns (n_kept, n_cancel, n_place).
#[cfg(test)]
fn diff_counts(current: &[CachedOrder], new_quotes: &[Quote]) -> (usize, usize, usize) {
    let (kept, cancel, place) = diff_orders(current, new_quotes);
    (kept.len(), cancel.len(), place.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn cached(id: u64, side: Side, price: Decimal, size: Decimal) -> CachedOrder {
        CachedOrder {
            order_id: id,
            side,
            price,
            size,
        }
    }

    fn quote(side: Side, price: Decimal, size: Decimal) -> Quote {
        Quote { side, price, size }
    }

    #[test]
    fn test_order_matches_quote_same_values() {
        let o = cached(1, Side::Bid, dec!(50000), dec!(0.1));
        let q = quote(Side::Bid, dec!(50000), dec!(0.1));
        assert!(order_matches_quote(&o, &q));
    }

    #[test]
    fn test_order_does_not_match_different_price() {
        let o = cached(1, Side::Bid, dec!(50000), dec!(0.1));
        let q = quote(Side::Bid, dec!(50001), dec!(0.1));
        assert!(!order_matches_quote(&o, &q));
    }

    #[test]
    fn test_order_does_not_match_different_side() {
        let o = cached(1, Side::Bid, dec!(50000), dec!(0.1));
        let q = quote(Side::Ask, dec!(50000), dec!(0.1));
        assert!(!order_matches_quote(&o, &q));
    }

    #[test]
    fn test_update_quotes_no_change_returns_same() {
        let orders = vec![
            cached(1, Side::Bid, dec!(49000), dec!(0.1)),
            cached(2, Side::Ask, dec!(51000), dec!(0.1)),
        ];
        let quotes = vec![
            quote(Side::Bid, dec!(49000), dec!(0.1)),
            quote(Side::Ask, dec!(51000), dec!(0.1)),
        ];
        let (kept, cancel, place) = diff_counts(&orders, &quotes);
        assert_eq!(kept, 2);
        assert_eq!(cancel, 0);
        assert_eq!(place, 0);
    }

    #[test]
    fn test_update_quotes_cancels_stale_and_places_new() {
        let orders = vec![
            cached(1, Side::Bid, dec!(49000), dec!(0.1)),
            cached(2, Side::Ask, dec!(51000), dec!(0.1)),
        ];
        let quotes = vec![
            quote(Side::Bid, dec!(49500), dec!(0.1)), // new price → cancel 1 + place
            quote(Side::Ask, dec!(51000), dec!(0.1)), // same → keep 2
        ];
        let (kept, cancel, place) = diff_counts(&orders, &quotes);
        assert_eq!(kept, 1);
        assert_eq!(cancel, 1);
        assert_eq!(place, 1);
    }

    #[test]
    fn test_cancel_all_when_quotes_empty() {
        let orders = vec![
            cached(1, Side::Bid, dec!(49000), dec!(0.1)),
            cached(2, Side::Ask, dec!(51000), dec!(0.1)),
        ];
        let quotes: Vec<Quote> = vec![];
        let (kept, cancel, place) = diff_counts(&orders, &quotes);
        assert_eq!(kept, 0);
        assert_eq!(cancel, 2);
        assert_eq!(place, 0);
    }

    #[test]
    fn test_chunking_over_max_atomic() {
        // 5 actions → 2 chunks (4 + 1)
        let actions: Vec<UserAtomicSubaction> = (0..5)
            .map(|i| UserAtomicSubaction::Cancel { order_id: i })
            .collect();
        let chunks: Vec<_> = actions.chunks(MAX_ATOMIC_ACTIONS).collect();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 4);
        assert_eq!(chunks[1].len(), 1);
    }

    #[test]
    fn test_extract_placed_orders_from_result() {
        use nord::proto::nord::receipt;

        // Simulate: [Cancel, Place(posted)] → should extract 1 placed order.
        let actions = vec![
            UserAtomicSubaction::Cancel { order_id: 100 },
            UserAtomicSubaction::Place {
                market_id: 1,
                side: Side::Bid,
                fill_mode: FillMode::PostOnly,
                is_reduce_only: false,
                size: Some(dec!(0.1)),
                price: Some(dec!(50000)),
                quote_size: None,
                client_order_id: None,
            },
        ];

        let results = vec![
            receipt::atomic_subaction_result_kind::Inner::CancelOrder(receipt::CancelOrderResult {
                order_id: 100,
                account_id: 1,
                client_order_id: None,
            }),
            receipt::atomic_subaction_result_kind::Inner::PlaceOrderResult(
                receipt::PlaceOrderResult {
                    posted: Some(receipt::Posted {
                        side: 0,
                        market_id: 1,
                        price: 5_000_000,
                        size: 1000,
                        order_id: 999,
                        account_id: 1,
                    }),
                    fills: vec![],
                    client_order_id: None,
                    sender_tracking_id: None,
                    triggered: None,
                },
            ),
        ];

        let placed = extract_placed_orders(&results, &actions);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].order_id, 999);
        assert_eq!(placed[0].side, Side::Bid);
        assert_eq!(placed[0].price, dec!(50000));
        assert_eq!(placed[0].size, dec!(0.1));
    }
}
