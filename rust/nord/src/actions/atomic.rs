use rust_decimal::Decimal;

use crate::error::{NordError, Result};
use crate::proto::nord;
use crate::rest::NordHttpClient;
use crate::types::{FillMode, QuoteSize, Side};
use crate::utils::{to_scaled_u64, to_scaled_u128};

use super::{create_action, send_action, SignFn};

/// An individual subaction within an atomic operation.
#[derive(Debug, Clone)]
pub enum AtomicSubaction {
    Place {
        market_id: u32,
        side: Side,
        fill_mode: FillMode,
        is_reduce_only: bool,
        size_decimals: u8,
        price_decimals: u8,
        size: Option<Decimal>,
        price: Option<Decimal>,
        quote_size: Option<QuoteSize>,
        client_order_id: Option<u64>,
    },
    Cancel {
        order_id: u64,
    },
}

/// User-friendly version that resolves market info automatically.
#[derive(Debug, Clone)]
pub enum UserAtomicSubaction {
    Place {
        market_id: u32,
        side: Side,
        fill_mode: FillMode,
        is_reduce_only: bool,
        size: Option<Decimal>,
        price: Option<Decimal>,
        quote_size: Option<QuoteSize>,
        client_order_id: Option<u64>,
    },
    Cancel {
        order_id: u64,
    },
}

/// Build protobuf atomic subactions from the typed versions.
pub fn build_atomic_subactions(
    actions: &[AtomicSubaction],
) -> Result<Vec<nord::AtomicSubactionKind>> {
    actions
        .iter()
        .map(|a| match a {
            AtomicSubaction::Place {
                market_id,
                side,
                fill_mode,
                is_reduce_only,
                size_decimals,
                price_decimals,
                size,
                price,
                quote_size,
                client_order_id,
            } => {
                let (wire_price, wire_size) = if let Some(qs) = quote_size {
                    qs.to_wire(*price_decimals as u32, *size_decimals as u32)
                } else {
                    let p = price.ok_or_else(|| {
                        NordError::Validation("place action requires price or quote_size".into())
                    })?;
                    let s = size.ok_or_else(|| {
                        NordError::Validation("place action requires size or quote_size".into())
                    })?;
                    (
                        to_scaled_u64(p, *price_decimals as u32),
                        to_scaled_u64(s, *size_decimals as u32),
                    )
                };

                let proto_side: i32 = match side {
                    Side::Ask => nord::Side::Ask as i32,
                    Side::Bid => nord::Side::Bid as i32,
                };
                let proto_fill_mode: i32 = fill_mode.to_proto() as i32;

                let proto_quote_size = quote_size.as_ref().map(|qs| {
                    let val = to_scaled_u128(
                        qs.value(),
                        (*price_decimals as u32) + (*size_decimals as u32),
                    );
                    nord::U128 {
                        lo: val as u64,
                        hi: (val >> 64) as u64,
                    }
                });

                Ok(nord::AtomicSubactionKind {
                    inner: Some(nord::atomic_subaction_kind::Inner::TradeOrPlace(
                        nord::TradeOrPlace {
                            market_id: *market_id,
                            order_type: Some(nord::OrderType {
                                side: proto_side,
                                fill_mode: proto_fill_mode,
                                is_reduce_only: *is_reduce_only,
                            }),
                            limit: Some(nord::OrderLimit {
                                price: wire_price,
                                size: wire_size,
                                quote_size: proto_quote_size,
                            }),
                            client_order_id: *client_order_id,
                        },
                    )),
                })
            }
            AtomicSubaction::Cancel { order_id } => Ok(nord::AtomicSubactionKind {
                inner: Some(nord::atomic_subaction_kind::Inner::CancelOrder(
                    nord::CancelOrder {
                        order_id: *order_id,
                    },
                )),
            }),
        })
        .collect()
}

/// Execute an atomic operation (up to 4 place/cancel actions).
pub async fn atomic(
    http_client: &NordHttpClient,
    sign_fn: &SignFn,
    timestamp: u64,
    nonce: u32,
    session_id: u64,
    account_id: u32,
    actions: &[AtomicSubaction],
) -> Result<(u64, Vec<nord::receipt::atomic_subaction_result_kind::Inner>)> {
    let subactions = build_atomic_subactions(actions)?;

    let kind = nord::action::Kind::Atomic(nord::Atomic {
        session_id,
        account_id: Some(account_id),
        actions: subactions,
    });

    let action = create_action(timestamp, nonce, kind);
    let receipt = send_action(http_client, &action, sign_fn).await?;

    match receipt.kind {
        Some(nord::receipt::Kind::Atomic(r)) => {
            let results: Vec<nord::receipt::atomic_subaction_result_kind::Inner> = r
                .results
                .into_iter()
                .filter_map(|r| r.inner)
                .collect();
            Ok((receipt.action_id, results))
        }
        Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
            "atomic failed: error code {code}"
        ))),
        _ => Err(NordError::ReceiptError(
            "unexpected receipt for atomic".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_build_cancel_subaction() {
        let actions = vec![AtomicSubaction::Cancel { order_id: 12345 }];
        let result = build_atomic_subactions(&actions).unwrap();
        assert_eq!(result.len(), 1);

        match &result[0].inner {
            Some(nord::atomic_subaction_kind::Inner::CancelOrder(c)) => {
                assert_eq!(c.order_id, 12345);
            }
            other => panic!("expected CancelOrder, got {other:?}"),
        }
    }

    #[test]
    fn test_build_place_subaction_with_price_and_size() {
        let actions = vec![AtomicSubaction::Place {
            market_id: 1,
            side: Side::Bid,
            fill_mode: FillMode::Limit,
            is_reduce_only: false,
            size_decimals: 4,
            price_decimals: 2,
            size: Some(dec!(0.5)),
            price: Some(dec!(50000.00)),
            quote_size: None,
            client_order_id: Some(99),
        }];
        let result = build_atomic_subactions(&actions).unwrap();
        assert_eq!(result.len(), 1);

        match &result[0].inner {
            Some(nord::atomic_subaction_kind::Inner::TradeOrPlace(top)) => {
                assert_eq!(top.market_id, 1);
                assert_eq!(top.client_order_id, Some(99));

                let ot = top.order_type.as_ref().unwrap();
                assert_eq!(ot.side, nord::Side::Bid as i32);
                assert_eq!(ot.fill_mode, nord::FillMode::Limit as i32);
                assert!(!ot.is_reduce_only);

                let limit = top.limit.as_ref().unwrap();
                // 50000.00 * 10^2 = 5_000_000
                assert_eq!(limit.price, 5_000_000);
                // 0.5 * 10^4 = 5000
                assert_eq!(limit.size, 5000);
                assert!(limit.quote_size.is_none());
            }
            other => panic!("expected TradeOrPlace, got {other:?}"),
        }
    }

    #[test]
    fn test_build_place_subaction_with_quote_size() {
        let qs = QuoteSize::new(dec!(30000.00), dec!(0.1));
        let actions = vec![AtomicSubaction::Place {
            market_id: 2,
            side: Side::Ask,
            fill_mode: FillMode::PostOnly,
            is_reduce_only: true,
            size_decimals: 4,
            price_decimals: 2,
            size: None,
            price: None,
            quote_size: Some(qs),
            client_order_id: None,
        }];
        let result = build_atomic_subactions(&actions).unwrap();
        assert_eq!(result.len(), 1);

        match &result[0].inner {
            Some(nord::atomic_subaction_kind::Inner::TradeOrPlace(top)) => {
                assert_eq!(top.market_id, 2);

                let ot = top.order_type.as_ref().unwrap();
                assert_eq!(ot.side, nord::Side::Ask as i32);
                assert_eq!(ot.fill_mode, nord::FillMode::PostOnly as i32);
                assert!(ot.is_reduce_only);

                let limit = top.limit.as_ref().unwrap();
                // from QuoteSize::to_wire: 30000.00 * 100 = 3_000_000, 0.1 * 10000 = 1000
                assert_eq!(limit.price, 3_000_000);
                assert_eq!(limit.size, 1000);

                // quote_size should be set: value = 30000.00 * 0.1 = 3000.00
                // scaled by 10^(2+4) = 10^6 => 3_000_000_000
                let qs_wire = limit.quote_size.as_ref().unwrap();
                assert_eq!(qs_wire.lo, 3_000_000_000u64);
                assert_eq!(qs_wire.hi, 0);
            }
            other => panic!("expected TradeOrPlace, got {other:?}"),
        }
    }

    #[test]
    fn test_build_mixed_cancel_and_place() {
        let actions = vec![
            AtomicSubaction::Cancel { order_id: 111 },
            AtomicSubaction::Place {
                market_id: 1,
                side: Side::Bid,
                fill_mode: FillMode::ImmediateOrCancel,
                is_reduce_only: false,
                size_decimals: 2,
                price_decimals: 2,
                size: Some(dec!(1.0)),
                price: Some(dec!(100.00)),
                quote_size: None,
                client_order_id: None,
            },
            AtomicSubaction::Cancel { order_id: 222 },
        ];
        let result = build_atomic_subactions(&actions).unwrap();
        assert_eq!(result.len(), 3);

        // First: cancel 111
        match &result[0].inner {
            Some(nord::atomic_subaction_kind::Inner::CancelOrder(c)) => {
                assert_eq!(c.order_id, 111);
            }
            other => panic!("expected CancelOrder, got {other:?}"),
        }

        // Second: place
        assert!(matches!(
            &result[1].inner,
            Some(nord::atomic_subaction_kind::Inner::TradeOrPlace(_))
        ));

        // Third: cancel 222
        match &result[2].inner {
            Some(nord::atomic_subaction_kind::Inner::CancelOrder(c)) => {
                assert_eq!(c.order_id, 222);
            }
            other => panic!("expected CancelOrder, got {other:?}"),
        }
    }

    #[test]
    fn test_build_place_missing_price_errors() {
        let actions = vec![AtomicSubaction::Place {
            market_id: 1,
            side: Side::Bid,
            fill_mode: FillMode::Limit,
            is_reduce_only: false,
            size_decimals: 4,
            price_decimals: 2,
            size: Some(dec!(1.0)),
            price: None,        // missing
            quote_size: None,   // also missing
            client_order_id: None,
        }];
        let err = build_atomic_subactions(&actions).unwrap_err();
        assert!(err.to_string().contains("price or quote_size"));
    }

    #[test]
    fn test_build_place_missing_size_errors() {
        let actions = vec![AtomicSubaction::Place {
            market_id: 1,
            side: Side::Bid,
            fill_mode: FillMode::Limit,
            is_reduce_only: false,
            size_decimals: 4,
            price_decimals: 2,
            size: None,            // missing
            price: Some(dec!(100)),
            quote_size: None,
            client_order_id: None,
        }];
        let err = build_atomic_subactions(&actions).unwrap_err();
        assert!(err.to_string().contains("size or quote_size"));
    }

    #[test]
    fn test_build_empty_actions() {
        let result = build_atomic_subactions(&[]).unwrap();
        assert!(result.is_empty());
    }
}
