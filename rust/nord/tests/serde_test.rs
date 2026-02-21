//! Integration tests for JSON round-trip serialization of key REST types.
//!
//! Each test constructs a realistic JSON fixture, deserializes it into the
//! Rust type, verifies field values, then re-serializes and deserializes again
//! to confirm the round-trip is lossless.

use nord::types::*;

// ---------------------------------------------------------------------------
// MarketInfo / MarketsInfo / TokenInfo
// ---------------------------------------------------------------------------

#[test]
fn test_markets_info_round_trip() {
    let json = r#"{
        "markets": [
            {
                "marketId": 1,
                "symbol": "BTCUSDC",
                "priceDecimals": 2,
                "sizeDecimals": 4,
                "baseTokenId": 0,
                "quoteTokenId": 1,
                "imf": 0.1,
                "mmf": 0.05,
                "cmf": 0.03
            },
            {
                "marketId": 2,
                "symbol": "ETHUSDC",
                "priceDecimals": 2,
                "sizeDecimals": 3,
                "baseTokenId": 2,
                "quoteTokenId": 1,
                "imf": 0.1,
                "mmf": 0.05,
                "cmf": 0.03
            }
        ],
        "tokens": [
            {
                "tokenId": 0,
                "symbol": "BTC",
                "decimals": 8,
                "mintAddr": "9n4nbM75f5Ui33ZbPYXn59EwSgE8CGsHtAeTH5YFeJ9E",
                "weightBps": 5000
            },
            {
                "tokenId": 1,
                "symbol": "USDC",
                "decimals": 6,
                "mintAddr": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
                "weightBps": 10000
            }
        ]
    }"#;

    let info: MarketsInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.markets.len(), 2);
    assert_eq!(info.tokens.len(), 2);
    assert_eq!(info.markets[0].symbol, "BTCUSDC");
    assert_eq!(info.markets[0].market_id, 1);
    assert_eq!(info.markets[0].price_decimals, 2);
    assert_eq!(info.markets[0].size_decimals, 4);
    assert_eq!(info.tokens[1].symbol, "USDC");
    assert_eq!(info.tokens[1].decimals, 6);

    // Round-trip
    let serialized = serde_json::to_string(&info).unwrap();
    let info2: MarketsInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(info2.markets.len(), 2);
    assert_eq!(info2.tokens[0].mint_addr, info.tokens[0].mint_addr);
}

// ---------------------------------------------------------------------------
// Account
// ---------------------------------------------------------------------------

#[test]
fn test_account_round_trip() {
    let json = r#"{
        "updateId": 42,
        "orders": [
            {
                "orderId": 100,
                "marketId": 1,
                "side": "bid",
                "size": 0.5,
                "price": 50000.0,
                "originalOrderSize": 1.0,
                "clientOrderId": 999
            }
        ],
        "positions": [
            {
                "marketId": 1,
                "openOrders": 3,
                "perp": {
                    "baseSize": 0.5,
                    "price": 50000.0,
                    "updatedFundingRateIndex": 0.001,
                    "fundingPaymentPnl": -5.0,
                    "sizePricePnl": 150.0,
                    "isLong": true
                },
                "actionId": 1234
            }
        ],
        "balances": [
            {
                "tokenId": 1,
                "token": "USDC",
                "amount": 10000.0
            }
        ],
        "margins": {
            "omf": 0.95,
            "mf": 0.9,
            "imf": 0.85,
            "cmf": 0.8,
            "mmf": 0.7,
            "pon": 10000.0,
            "pn": 9500.0,
            "bankruptcy": false
        }
    }"#;

    let acct: Account = serde_json::from_str(json).unwrap();
    assert_eq!(acct.update_id, 42);
    assert_eq!(acct.orders.len(), 1);
    assert_eq!(acct.orders[0].order_id, 100);
    assert_eq!(acct.orders[0].side, Side::Bid);
    assert_eq!(acct.orders[0].client_order_id, Some(999));
    assert_eq!(acct.positions.len(), 1);
    let perp = acct.positions[0].perp.as_ref().unwrap();
    assert!(perp.is_long);
    assert_eq!(perp.base_size, 0.5);
    assert_eq!(acct.balances[0].token, "USDC");
    assert!(!acct.margins.bankruptcy);

    // Round-trip
    let serialized = serde_json::to_string(&acct).unwrap();
    let acct2: Account = serde_json::from_str(&serialized).unwrap();
    assert_eq!(acct2.update_id, acct.update_id);
    assert_eq!(acct2.orders[0].order_id, acct.orders[0].order_id);
}

// ---------------------------------------------------------------------------
// OrderbookInfo
// ---------------------------------------------------------------------------

#[test]
fn test_orderbook_info_round_trip() {
    let json = r#"{
        "updateId": 555,
        "asks": [[50100.0, 0.5], [50200.0, 1.0], [50300.0, 2.0]],
        "bids": [[49900.0, 0.3], [49800.0, 1.5]],
        "asksSummary": {"sum": 3.5, "count": 3},
        "bidsSummary": {"sum": 1.8, "count": 2}
    }"#;

    let ob: OrderbookInfo = serde_json::from_str(json).unwrap();
    assert_eq!(ob.update_id, 555);
    assert_eq!(ob.asks.len(), 3);
    assert_eq!(ob.bids.len(), 2);
    assert_eq!(ob.asks[0], [50100.0, 0.5]);
    assert_eq!(ob.asks_summary.count, 3);
    assert_eq!(ob.bids_summary.sum, 1.8);

    // Round-trip
    let serialized = serde_json::to_string(&ob).unwrap();
    let ob2: OrderbookInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(ob2.asks.len(), ob.asks.len());
}

// ---------------------------------------------------------------------------
// Trade
// ---------------------------------------------------------------------------

#[test]
fn test_trade_round_trip() {
    let json = r#"{
        "time": "2024-06-15T12:30:45.123Z",
        "actionId": 10001,
        "tradeId": 5001,
        "takerId": 100,
        "takerSide": "ask",
        "makerId": 200,
        "marketId": 1,
        "orderId": 9999,
        "price": 50123.45,
        "baseSize": 0.25
    }"#;

    let trade: Trade = serde_json::from_str(json).unwrap();
    assert_eq!(trade.time, "2024-06-15T12:30:45.123Z");
    assert_eq!(trade.action_id, 10001);
    assert_eq!(trade.taker_side, Side::Ask);
    assert_eq!(trade.price, 50123.45);
    assert_eq!(trade.base_size, 0.25);

    let serialized = serde_json::to_string(&trade).unwrap();
    let trade2: Trade = serde_json::from_str(&serialized).unwrap();
    assert_eq!(trade2.trade_id, trade.trade_id);
}

// ---------------------------------------------------------------------------
// MarketStats
// ---------------------------------------------------------------------------

#[test]
fn test_market_stats_round_trip() {
    let json = r#"{
        "indexPrice": 50000.0,
        "indexPriceConf": 5.0,
        "frozen": false,
        "volumeBase24h": 1234.56,
        "volumeQuote24h": 61728000.0,
        "high24h": 51000.0,
        "low24h": 49000.0,
        "close24h": 50500.0,
        "prevClose24h": 49800.0,
        "perpStats": {
            "mark_price": 50010.0,
            "aggregated_funding_index": 0.00123,
            "funding_rate": 0.0001,
            "next_funding_time": "2024-06-15T13:00:00Z",
            "open_interest": 500.0
        }
    }"#;

    let stats: MarketStats = serde_json::from_str(json).unwrap();
    assert_eq!(stats.index_price, Some(50000.0));
    assert_eq!(stats.frozen, Some(false));
    assert_eq!(stats.volume_base24h, 1234.56);
    let perp = stats.perp_stats.as_ref().unwrap();
    assert_eq!(perp.mark_price, Some(50010.0));
    assert_eq!(perp.open_interest, 500.0);

    let serialized = serde_json::to_string(&stats).unwrap();
    let stats2: MarketStats = serde_json::from_str(&serialized).unwrap();
    assert_eq!(stats2.volume_quote24h, stats.volume_quote24h);
}

// ---------------------------------------------------------------------------
// MarketStats with nulls
// ---------------------------------------------------------------------------

#[test]
fn test_market_stats_with_nulls() {
    let json = r#"{
        "indexPrice": null,
        "indexPriceConf": null,
        "frozen": null,
        "volumeBase24h": 0.0,
        "volumeQuote24h": 0.0,
        "high24h": null,
        "low24h": null,
        "close24h": null,
        "prevClose24h": null,
        "perpStats": null
    }"#;

    let stats: MarketStats = serde_json::from_str(json).unwrap();
    assert!(stats.index_price.is_none());
    assert!(stats.perp_stats.is_none());
    assert!(stats.frozen.is_none());
}

// ---------------------------------------------------------------------------
// User
// ---------------------------------------------------------------------------

#[test]
fn test_user_round_trip() {
    let json = r#"{
        "accountIds": [0, 1, 2],
        "sessions": {
            "abc123pubkey": {
                "pubkey": "abc123pubkey",
                "expiry": "2024-12-31T23:59:59Z"
            }
        }
    }"#;

    let user: User = serde_json::from_str(json).unwrap();
    assert_eq!(user.account_ids, vec![0, 1, 2]);
    assert_eq!(user.sessions.len(), 1);
    let session = user.sessions.get("abc123pubkey").unwrap();
    assert_eq!(session.expiry, "2024-12-31T23:59:59Z");

    let serialized = serde_json::to_string(&user).unwrap();
    let user2: User = serde_json::from_str(&serialized).unwrap();
    assert_eq!(user2.account_ids, user.account_ids);
}

// ---------------------------------------------------------------------------
// WithdrawalInfo
// ---------------------------------------------------------------------------

#[test]
fn test_withdrawal_info_round_trip() {
    let json = r#"{
        "time": "2024-06-15T10:00:00Z",
        "actionId": 7001,
        "accountId": 5,
        "tokenId": 1,
        "amount": 500.0,
        "balance": 9500.0,
        "fee": 0.5,
        "destPubkey": "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"
    }"#;

    let w: WithdrawalInfo = serde_json::from_str(json).unwrap();
    assert_eq!(w.action_id, 7001);
    assert_eq!(w.amount, 500.0);
    assert_eq!(w.fee, 0.5);
    assert!(w.dest_pubkey.is_some());

    let serialized = serde_json::to_string(&w).unwrap();
    let w2: WithdrawalInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(w2.dest_pubkey, w.dest_pubkey);
}

#[test]
fn test_withdrawal_info_null_pubkey() {
    let json = r#"{
        "time": "2024-06-15T10:00:00Z",
        "actionId": 7002,
        "accountId": 5,
        "tokenId": 1,
        "amount": 100.0,
        "balance": 9400.0,
        "fee": 0.1,
        "destPubkey": null
    }"#;

    let w: WithdrawalInfo = serde_json::from_str(json).unwrap();
    assert!(w.dest_pubkey.is_none());
}

// ---------------------------------------------------------------------------
// DepositInfo
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_info_round_trip() {
    let json = r#"{
        "time": "2024-06-14T08:00:00Z",
        "actionId": 6001,
        "accountId": 3,
        "tokenId": 1,
        "amount": 10000.0,
        "balance": 10000.0,
        "eventIndex": 42
    }"#;

    let d: DepositInfo = serde_json::from_str(json).unwrap();
    assert_eq!(d.action_id, 6001);
    assert_eq!(d.amount, 10000.0);
    assert_eq!(d.event_index, 42);

    let serialized = serde_json::to_string(&d).unwrap();
    let d2: DepositInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(d2.event_index, d.event_index);
}

// ---------------------------------------------------------------------------
// TriggerInfo
// ---------------------------------------------------------------------------

#[test]
fn test_trigger_info_round_trip() {
    let json = r#"{
        "accountId": 5,
        "marketId": 1,
        "triggerPrice": 45000,
        "limitPrice": 44900,
        "side": "bid",
        "kind": "stopLoss",
        "actionId": 8001,
        "createdAt": "2024-06-15T09:00:00Z"
    }"#;

    let t: TriggerInfo = serde_json::from_str(json).unwrap();
    assert_eq!(t.account_id, 5);
    assert_eq!(t.trigger_price, 45000);
    assert_eq!(t.limit_price, Some(44900));
    assert_eq!(t.side, Side::Bid);
    assert_eq!(t.kind, TriggerKind::StopLoss);

    let serialized = serde_json::to_string(&t).unwrap();
    let t2: TriggerInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(t2.trigger_price, t.trigger_price);
}

#[test]
fn test_trigger_info_null_limit_price() {
    let json = r#"{
        "accountId": 5,
        "marketId": 1,
        "triggerPrice": 55000,
        "limitPrice": null,
        "side": "ask",
        "kind": "takeProfit",
        "actionId": 8002,
        "createdAt": "2024-06-15T09:00:00Z"
    }"#;

    let t: TriggerInfo = serde_json::from_str(json).unwrap();
    assert!(t.limit_price.is_none());
    assert_eq!(t.kind, TriggerKind::TakeProfit);
}

// ---------------------------------------------------------------------------
// AccountPnlInfo
// ---------------------------------------------------------------------------

#[test]
fn test_account_pnl_info_round_trip() {
    let json = r#"{
        "time": "2024-06-15T12:00:00Z",
        "actionId": 9001,
        "marketId": 1,
        "tradingPnl": 250.50,
        "settledFundingPnl": -10.25,
        "positionSize": 0.5
    }"#;

    let pnl: AccountPnlInfo = serde_json::from_str(json).unwrap();
    assert_eq!(pnl.trading_pnl, 250.50);
    assert_eq!(pnl.settled_funding_pnl, -10.25);

    let serialized = serde_json::to_string(&pnl).unwrap();
    let pnl2: AccountPnlInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(pnl2.trading_pnl, pnl.trading_pnl);
}

// ---------------------------------------------------------------------------
// AccountVolumeInfo
// ---------------------------------------------------------------------------

#[test]
fn test_account_volume_info_round_trip() {
    let json = r#"{
        "marketId": 1,
        "volumeBase": 100.5,
        "volumeQuote": 5025000.0
    }"#;

    let vol: AccountVolumeInfo = serde_json::from_str(json).unwrap();
    assert_eq!(vol.market_id, 1);
    assert_eq!(vol.volume_base, 100.5);

    let serialized = serde_json::to_string(&vol).unwrap();
    let vol2: AccountVolumeInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(vol2.volume_quote, vol.volume_quote);
}

// ---------------------------------------------------------------------------
// AccountFundingInfo
// ---------------------------------------------------------------------------

#[test]
fn test_account_funding_info_round_trip() {
    let json = r#"{
        "time": "2024-06-15T13:00:00Z",
        "actionId": 11001,
        "marketId": 1,
        "positionSize": 0.5,
        "fundingPnl": -2.50
    }"#;

    let f: AccountFundingInfo = serde_json::from_str(json).unwrap();
    assert_eq!(f.funding_pnl, -2.50);

    let serialized = serde_json::to_string(&f).unwrap();
    let f2: AccountFundingInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(f2.funding_pnl, f.funding_pnl);
}

// ---------------------------------------------------------------------------
// LiquidationInfo
// ---------------------------------------------------------------------------

#[test]
fn test_liquidation_info_round_trip() {
    let json = r#"{
        "time": "2024-06-15T14:00:00Z",
        "actionId": 12001,
        "liquidatorId": 1,
        "liquidateeId": 2,
        "fee": 5.0,
        "liquidationKind": "place_order",
        "marketId": 1,
        "tokenId": null,
        "orderId": 50000,
        "orderPrice": 48000.0,
        "orderSize": 0.1,
        "orderQuote": 4800.0,
        "preOmf": 0.01,
        "preMmf": 0.02,
        "preImf": 0.03,
        "preCmf": 0.04,
        "prePon": 100.0,
        "preMf": 0.05,
        "prePn": 95.0,
        "postOmf": 0.10,
        "postMmf": 0.12,
        "postImf": 0.15,
        "postCmf": 0.18,
        "postPon": 200.0,
        "postMf": 0.20,
        "postPn": 190.0
    }"#;

    let liq: LiquidationInfo = serde_json::from_str(json).unwrap();
    assert_eq!(liq.liquidator_id, 1);
    assert_eq!(liq.liquidatee_id, 2);
    assert_eq!(liq.liquidation_kind, LiquidationKind::PlaceOrder);
    assert_eq!(liq.market_id, Some(1));
    assert!(liq.token_id.is_none());
    assert_eq!(liq.order_price, Some(48000.0));

    let serialized = serde_json::to_string(&liq).unwrap();
    let liq2: LiquidationInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(liq2.fee, liq.fee);
}

// ---------------------------------------------------------------------------
// FeeTierConfig / AccountFeeTier
// ---------------------------------------------------------------------------

#[test]
fn test_fee_tier_config_round_trip() {
    let json = r#"{"maker_fee_ppm": 100, "taker_fee_ppm": 300}"#;

    let tier: FeeTierConfig = serde_json::from_str(json).unwrap();
    assert_eq!(tier.maker_fee_ppm, 100);
    assert_eq!(tier.taker_fee_ppm, 300);

    let serialized = serde_json::to_string(&tier).unwrap();
    let tier2: FeeTierConfig = serde_json::from_str(&serialized).unwrap();
    assert_eq!(tier2.maker_fee_ppm, tier.maker_fee_ppm);
}

#[test]
fn test_account_fee_tier_round_trip() {
    let json = r#"{"accountId": 5, "feeTier": 2}"#;

    let aft: AccountFeeTier = serde_json::from_str(json).unwrap();
    assert_eq!(aft.account_id, 5);
    assert_eq!(aft.fee_tier, 2);
}

// ---------------------------------------------------------------------------
// AdminInfo
// ---------------------------------------------------------------------------

#[test]
fn test_admin_info_round_trip() {
    let json = r#"{"key": "ABCDpubkey", "roles": ["admin", "fee_manager"]}"#;

    let info: AdminInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.key, "ABCDpubkey");
    assert_eq!(info.roles, vec!["admin", "fee_manager"]);

    let serialized = serde_json::to_string(&info).unwrap();
    let info2: AdminInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(info2.roles, info.roles);
}

// ---------------------------------------------------------------------------
// ActionsItem
// ---------------------------------------------------------------------------

#[test]
fn test_actions_item_round_trip() {
    let json = r#"{
        "actionId": 999,
        "physicalTime": "2024-06-15T15:00:00Z",
        "payload": "0a0b0c0d"
    }"#;

    let item: ActionsItem = serde_json::from_str(json).unwrap();
    assert_eq!(item.action_id, 999);
    assert_eq!(item.physical_time, "2024-06-15T15:00:00Z");
    assert_eq!(item.payload, "0a0b0c0d");

    let serialized = serde_json::to_string(&item).unwrap();
    let item2: ActionsItem = serde_json::from_str(&serialized).unwrap();
    assert_eq!(item2.action_id, item.action_id);
}

// ---------------------------------------------------------------------------
// PageResult<Trade>
// ---------------------------------------------------------------------------

#[test]
fn test_page_result_round_trip() {
    let json = r#"{
        "items": [
            {
                "time": "2024-06-15T12:30:45.123Z",
                "actionId": 10001,
                "tradeId": 5001,
                "takerId": 100,
                "takerSide": "ask",
                "makerId": 200,
                "marketId": 1,
                "orderId": 9999,
                "price": 50123.45,
                "baseSize": 0.25
            }
        ],
        "nextStartInclusive": 10002
    }"#;

    let page: PageResult<Trade> = serde_json::from_str(json).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].trade_id, 5001);
    assert!(page.next_start_inclusive.is_some());

    let serialized = serde_json::to_string(&page).unwrap();
    let page2: PageResult<Trade> = serde_json::from_str(&serialized).unwrap();
    assert_eq!(page2.items.len(), 1);
}

#[test]
fn test_page_result_empty() {
    let json = r#"{"items": [], "nextStartInclusive": null}"#;

    let page: PageResult<Trade> = serde_json::from_str(json).unwrap();
    assert!(page.items.is_empty());
    assert!(page.next_start_inclusive.is_none());
}

// ---------------------------------------------------------------------------
// TokenStats
// ---------------------------------------------------------------------------

#[test]
fn test_token_stats_round_trip() {
    let json = r#"{
        "symbol": "USDC",
        "decimals": 6,
        "mintAddr": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "weightBps": 10000,
        "oracleSymbol": "USDC",
        "indexPrice": {
            "median": 1.0001,
            "confidence": 0.0002
        }
    }"#;

    let ts: TokenStats = serde_json::from_str(json).unwrap();
    assert_eq!(ts.symbol, "USDC");
    assert_eq!(ts.decimals, 6);
    let price = ts.index_price.as_ref().unwrap();
    assert_eq!(price.median, 1.0001);
    assert_eq!(price.confidence, 0.0002);

    let serialized = serde_json::to_string(&ts).unwrap();
    let ts2: TokenStats = serde_json::from_str(&serialized).unwrap();
    assert_eq!(ts2.weight_bps, ts.weight_bps);
}

#[test]
fn test_token_stats_null_price() {
    let json = r#"{
        "symbol": "BTC",
        "decimals": 8,
        "mintAddr": "9n4nbM75f5Ui33ZbPYXn59EwSgE8CGsHtAeTH5YFeJ9E",
        "weightBps": 5000,
        "oracleSymbol": "BTC",
        "indexPrice": null
    }"#;

    let ts: TokenStats = serde_json::from_str(json).unwrap();
    assert!(ts.index_price.is_none());
}

// ---------------------------------------------------------------------------
// OrderInfo
// ---------------------------------------------------------------------------

#[test]
fn test_order_info_round_trip() {
    let json = r#"{
        "addedAt": "2024-06-15T10:00:00Z",
        "updatedAt": "2024-06-15T10:05:00Z",
        "tradeId": 3001,
        "traderId": 10,
        "marketId": 1,
        "orderId": 7777,
        "side": "ask",
        "placedSize": 1.0,
        "filledSize": 0.5,
        "updateActionId": 5001,
        "isReduceOnly": false,
        "fillMode": "Limit",
        "placedPrice": 50000.0,
        "originalSizeLimit": 1.0,
        "originalPriceLimit": 50000.0,
        "placementOrigin": "User",
        "finalizationReason": "Filled",
        "marketSymbol": "BTCUSDC",
        "tokenSymbol": "USDC"
    }"#;

    let order: OrderInfo = serde_json::from_str(json).unwrap();
    assert_eq!(order.order_id, 7777);
    assert_eq!(order.side, Side::Ask);
    assert_eq!(order.filled_size, Some(0.5));
    assert_eq!(order.fill_mode, FillMode::Limit);
    assert_eq!(order.placement_origin, PlacementOrigin::User);
    assert_eq!(order.finalization_reason, Some(FinalizationReason::Filled));

    let serialized = serde_json::to_string(&order).unwrap();
    let order2: OrderInfo = serde_json::from_str(&serialized).unwrap();
    assert_eq!(order2.order_id, order.order_id);
}

// ---------------------------------------------------------------------------
// Enum serialization
// ---------------------------------------------------------------------------

#[test]
fn test_side_serde() {
    assert_eq!(serde_json::to_string(&Side::Ask).unwrap(), "\"ask\"");
    assert_eq!(serde_json::to_string(&Side::Bid).unwrap(), "\"bid\"");
    assert_eq!(serde_json::from_str::<Side>("\"ask\"").unwrap(), Side::Ask);
    assert_eq!(serde_json::from_str::<Side>("\"bid\"").unwrap(), Side::Bid);
}

#[test]
fn test_fill_role_serde() {
    assert_eq!(serde_json::to_string(&FillRole::Maker).unwrap(), "\"maker\"");
    assert_eq!(serde_json::to_string(&FillRole::Taker).unwrap(), "\"taker\"");
}

#[test]
fn test_trigger_kind_serde() {
    assert_eq!(
        serde_json::to_string(&TriggerKind::StopLoss).unwrap(),
        "\"stopLoss\""
    );
    assert_eq!(
        serde_json::to_string(&TriggerKind::TakeProfit).unwrap(),
        "\"takeProfit\""
    );
}

#[test]
fn test_candle_resolution_serde() {
    assert_eq!(
        serde_json::to_string(&CandleResolution::OneMinute).unwrap(),
        "\"1\""
    );
    assert_eq!(
        serde_json::to_string(&CandleResolution::OneDay).unwrap(),
        "\"1D\""
    );
    assert_eq!(
        serde_json::from_str::<CandleResolution>("\"15\"").unwrap(),
        CandleResolution::FifteenMinutes
    );
}

#[test]
fn test_liquidation_kind_serde() {
    assert_eq!(
        serde_json::to_string(&LiquidationKind::PlaceOrder).unwrap(),
        "\"place_order\""
    );
    assert_eq!(
        serde_json::to_string(&LiquidationKind::CancelOrder).unwrap(),
        "\"cancel_order\""
    );
    assert_eq!(
        serde_json::to_string(&LiquidationKind::Bankruptcy).unwrap(),
        "\"bankruptcy\""
    );
}
