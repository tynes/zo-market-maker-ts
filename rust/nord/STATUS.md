# Nord Rust SDK - Implementation Status

## Completed (Phases 1-9)

All code compiles cleanly with zero errors and zero clippy warnings.

### Phase 1: Workspace + Protobuf
- Restructured `rust/` into Cargo workspace with `zo` (binary) and `nord` (library) crates
- Downloaded proto schema from `https://zo-mainnet.n1.xyz/schema.proto` → `rust/nord/proto/nord.proto`
- Set up `prost-build` in `build.rs` (requires `PROTOC=/tmp/protoc/bin/protoc`)

### Phase 2: Core Types + Error
- `src/error.rs` - NordError enum with thiserror
- `src/config.rs` - NordConfig struct
- `src/types/` - 18 sub-modules: enums, market, account, orderbook, stats, user, trade, pnl, trigger, withdrawal, fee, admin, volume, page, action, quote_size, deposit, liquidation, funding
- `src/utils.rs` - Scaling functions, hex decode, keypair parsing, market/token lookup

### Phase 3: REST Client
- `src/rest/mod.rs` - NordHttpClient (reqwest wrapper)
- `src/rest/endpoints.rs` - ~30 REST endpoints matching the TypeScript SDK

### Phase 4: Actions + Signing
- `src/actions/mod.rs` - create_action, prepare_action, send_action, SignFn type
- `src/actions/signing.rs` - hex-encoded, raw, and Solana transaction framing sign schemes
- `src/actions/session.rs` - create_session, revoke_session
- `src/actions/atomic.rs` - AtomicSubaction, build_atomic_subactions, atomic execution

### Phase 5: Nord Client
- `src/client.rs` - Nord struct with HTTP client, market/token info, symbol resolution, REST delegates, WebSocket factory

### Phase 6: WebSocket Client
- `src/ws/events.rs` - WebSocketTradeUpdate, WebSocketDeltaUpdate, WebSocketAccountUpdate, WebSocketCandleUpdate
- `src/ws/mod.rs` - NordWebSocketClient with auto-reconnect, ping/pong heartbeat, broadcast dispatch
- `src/ws/subscriber.rs` - Typed subscription wrappers (OrderbookSubscription, TradeSubscription, etc.)

### Phase 7: NordUser
- `src/user.rs` - Session management, place_order, cancel_order, cancel_order_by_client_id, atomic, add_trigger, remove_trigger, transfer_to_account, withdraw

### Phase 8: Solana On-Chain Operations
- `src/solana.rs` - get_ata, deposit (feature-gated behind `solana` feature)

### Phase 9: NordAdmin
- `src/admin.rs` - update_acl, create_token, create_market, pyth_set_wormhole_guardians, pyth_set_symbol_feed, pause, unpause, freeze_market, unfreeze_market, add_fee_tier, update_fee_tier, update_accounts_tier, fee_vault_transfer

## Remaining: Phase 10 - Integration + Polish + Tests

### Unit tests (inline `#[cfg(test)]` modules):
- **Type serde tests**: Round-trip JSON ser/de for every REST type against captured JSON fixtures
- **Utility tests**: `to_scaled_u64`/`to_scaled_u128` edge cases, `is_rfc3339`, `decode_hex`, `find_market`/`find_token`
- **Proto tests**: Encode/decode Action variants, `decode_length_delimited`
- **Action tests**: Build PlaceOrder, Atomic with mixed cancel+place, QuoteSize.to_wire()

### Integration tests (`rust/nord/tests/`):
- **REST client** (`client_test.rs`): wiremock mock server returning captured JSON responses
- **WebSocket** (`ws_test.rs`): Local tokio-tungstenite server replaying captured messages; test reconnect, heartbeat
- **User lifecycle** (`user_test.rs`): Mock HTTP+WS, test create session → place order → cancel → atomic
- **Signing** (`signing_test.rs`): Known keypairs, verify signatures match TS implementation

### Test data:
- Capture real responses from the nord server and save as JSON fixtures in `tests/fixtures/`

### Other polish:
- Wire up `src/lib.rs` with any missing public re-exports
- Doc comments matching TypeScript JSDoc
- Make `zo` crate optionally depend on `nord`

## Key Proto Type Mappings (gotchas found during implementation)

| What we assumed | What prost actually generates |
|---|---|
| `Action.timestamp: u64` | `Action.current_timestamp: i64` |
| `Action.nonce: u64` | `Action.nonce: u32` |
| `receipt::Kind::Error { message, code }` | `receipt::Kind::Err(i32)` (Error enum code) |
| `CancelOrder` | `CancelOrderById` |
| `TransferToAccount` | `Transfer` (with `Recipient` oneof) |
| `InsertToken` / `InsertMarket` | `CreateToken` / `CreateMarket` |
| `UpdateGuardianSet` | `PythSetWormholeGuardians` |
| `OracleSymbolFeed` | `PythSetSymbolFeed` |
| `FreezeMarket { frozen: bool }` | Separate `FreezeMarket` / `UnfreezeMarket` |
| `PlaceOrder.account_id: u32` | `PlaceOrder.sender_account_id: Option<u32>` |
| `PlaceOrderResult.order_id` | `PlaceOrderResult.posted: Option<Posted>` (order_id inside) |
| `Atomic.subactions` | `Atomic.actions: Vec<AtomicSubactionKind>` |
| `AtomicSubaction { kind: Place/Cancel }` | `AtomicSubactionKind { inner: TradeOrPlace/CancelOrder }` |
| `TradeOrPlace` flat fields | Uses `OrderType` + `OrderLimit` sub-messages |
| `AddTrigger` flat side/kind/price | Uses `TriggerKey` + `TriggerPrices` sub-messages |
| Admin actions no auth field | All require `acl_pubkey: Vec<u8>` |
| `UpdateAcl { add_roles, remove_roles }` | `UpdateAcl { roles_mask, roles_value }` |
| `receipt::SessionCreated` | `receipt::CreateSessionResult` |
| `receipt::TransferToAccountResult` | `receipt::Transferred` |

## Build Commands
```bash
PROTOC=/tmp/protoc/bin/protoc cargo build -p nord    # build
PROTOC=/tmp/protoc/bin/protoc cargo clippy -p nord   # lint
PROTOC=/tmp/protoc/bin/protoc cargo test -p nord     # test
PROTOC=/tmp/protoc/bin/protoc cargo build -p zo      # verify zo still works
```
