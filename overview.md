# ZO Market Maker - Codebase Overview

## What Is This?

ZO Market Maker is a TypeScript-based automated market-making bot for the **01 Exchange** (Zo Protocol), a perpetual futures trading platform built on Solana. It places bid and ask orders around a calculated fair price, profiting from the spread, and automatically reduces exposure when positions grow too large.

## Project Structure

```
zo-market-maker-ts/
├── src/
│   ├── bots/mm/
│   │   ├── index.ts          # MarketMaker class — core orchestration
│   │   ├── config.ts         # Configuration types and defaults
│   │   ├── quoter.ts         # Quote generation with tick/lot alignment
│   │   └── position.ts       # Position tracking and close-mode logic
│   ├── cli/
│   │   ├── bot.ts            # CLI entry point for the market maker
│   │   └── monitor.ts        # TUI market monitor (blessed)
│   ├── sdk/
│   │   ├── client.ts         # 01 Exchange SDK client wrapper
│   │   ├── account.ts        # Account stream (fills, orders, cancels)
│   │   ├── orders.ts         # Order operations (place, cancel, atomic)
│   │   └── orderbook.ts      # Orderbook stream with staleness detection
│   ├── pricing/
│   │   ├── binance.ts        # Binance Futures WebSocket price feed
│   │   └── fair-price.ts     # Fair price calculator (offset-median)
│   ├── utils/
│   │   └── logger.ts         # Structured logging system
│   └── types.ts              # Shared type definitions
├── Dockerfile                # Multi-stage build (Node 25-slim)
├── docker-compose.yml        # Docker Compose for deployment
├── .dockerignore
├── .env.example              # Environment variable template
├── package.json
├── tsconfig.json
└── biome.json                # Biome formatter/linter config
```

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                     MarketMaker Bot                       │
│                                                           │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ Pricing Pipeline                                     │ │
│  │  Binance Futures WS ──┐                              │ │
│  │                        ├─→ FairPriceCalculator        │ │
│  │  01 Orderbook WS ─────┘   (offset-median, 5min)     │ │
│  └─────────────────────────────────────────────────────┘ │
│                            ↓                              │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ Strategy (throttled to 100ms)                        │ │
│  │  PositionTracker → allowed sides + mode              │ │
│  │  Quoter → aligned bid/ask at fair ± spread           │ │
│  └─────────────────────────────────────────────────────┘ │
│                            ↓                              │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ Execution                                            │ │
│  │  Atomic order ops (cancel + place, max 4 per call)   │ │
│  │  Smart update: skip if quotes unchanged              │ │
│  └─────────────────────────────────────────────────────┘ │
│                            ↓                              │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ 01 Exchange SDK (@n1xyz/nord-ts)                     │ │
│  │  WebSocket subscriptions + REST snapshots            │ │
│  │  Solana on-chain transactions                        │ │
│  └─────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

## Core Components

### MarketMaker (`src/bots/mm/index.ts`)

The central orchestrator. It initializes all streams, wires up event handlers, and runs the main quote-update loop. Lifecycle:

1. Initialize SDK client (Solana RPC + 01 Exchange)
2. Set up event handlers for account, orderbook, and Binance streams
3. Sync initial orders from the server
4. Start periodic intervals (status logging, order sync, position sync)
5. Register SIGINT/SIGTERM shutdown handlers
6. Wait forever (event-driven)

On shutdown, it cancels all open orders atomically and closes all streams.

### FairPriceCalculator (`src/pricing/fair-price.ts`)

Calculates a fair price using offset-median:

```
fairPrice = binanceMid + median(01Mid - binanceMid)
```

- Maintains a circular buffer of up to 500 offset samples (~8 min)
- Samples taken once per second from valid price pairs within 1-second freshness
- Uses a configurable time window (default 5 min)
- Requires a warmup period (`warmupSeconds`, default 10) before producing prices
- Median is robust against flash crashes and outliers

### Quoter (`src/bots/mm/quoter.ts`)

Generates bid/ask quotes from a quoting context:

- **Bid**: `fairPrice - (fairPrice * spreadBps / 10000)`, rounded down to tick size, clamped below best ask
- **Ask**: `fairPrice + (fairPrice * spreadBps / 10000)`, rounded up to tick size, clamped above best bid
- Size aligned to lot size
- In close mode, limits order size to current position size
- Returns empty array if calculated size is too small

### PositionTracker (`src/bots/mm/position.ts`)

Tracks the bot's current position and determines trading mode:

- **Normal mode** (position < `closeThresholdUsd`): quote both sides at `spreadBps`
- **Close mode** (position >= `closeThresholdUsd`): quote only the reducing side at `takeProfitBps`
- Optimistic fill updates from WebSocket events
- Periodic server sync (every 5 seconds) with drift warnings

### BinancePriceFeed (`src/pricing/binance.ts`)

WebSocket client for Binance Futures `bookTicker` stream:

- Keep-alive via ping/pong (30-second interval, 10-second timeout)
- Staleness detection (60-second threshold, 10-second check interval)
- Auto-reconnect with 3-second delay on disconnect or stale detection

### ZoOrderbookStream (`src/sdk/orderbook.ts`)

01 Exchange orderbook with two-phase initialization:

1. Subscribe to WebSocket (start buffering deltas)
2. Fetch REST snapshot (baseline state)
3. Apply buffered deltas with `update_id > snapshotId`

Maintains sorted bids (descending) and asks (ascending), trimmed to 100 levels per side. Staleness detection triggers full reconnect + re-sync.

### AccountStream (`src/sdk/account.ts`)

WebSocket subscription for account events (fills, placements, cancels). Maintains a local order map and triggers fill callbacks. On reconnect, re-syncs orders from server to prevent desync.

### Order Operations (`src/sdk/orders.ts`)

Atomic transaction-based order management:

- Place and cancel batched into atomic calls (max 4 actions per call)
- All placements are post-only (no crossing the spread)
- Smart update: compares new quotes vs current orders, only modifies what changed
- Returns placed order IDs immediately (confirmed later via WebSocket)

### SDK Client (`src/sdk/client.ts`)

Wrapper around `@n1xyz/nord-ts` (official 01 Exchange SDK). Handles connection, initialization, and user account loading from a base58-encoded private key. Hardcoded to mainnet.

## Trading Strategy

```
1. WARM UP (10 seconds)
   Collect price samples from Binance and 01 Exchange
   Calculate median offset between exchanges
   Do not quote until warmup completes

2. CALCULATE FAIR PRICE
   fairPrice = binanceMid + median(01Mid - binanceMid)
   Throttled to 100ms minimum update interval

3. DETERMINE MODE
   If position < closeThresholdUsd → Normal Mode
     Quote both sides at spreadBps (default 8 bps)
   If position >= closeThresholdUsd → Close Mode
     Quote only reducing side at takeProfitBps (default 0.1 bps)
     Order size = current position size

4. GENERATE QUOTES
   Bid/ask at fair ± spread, aligned to tick/lot sizes
   Clamped to BBO (never cross the spread)

5. EXECUTE
   Compare vs current orders, cancel stale, place new
   Atomic transactions (cancel + place in single call)

6. TRACK
   Optimistic position updates on fills
   Periodic server sync for drift detection
```

## Configuration

### MarketMakerConfig (`src/bots/mm/config.ts`)

| Parameter | Default | Description |
|---|---|---|
| `symbol` | CLI arg | Market symbol (`"BTC"` or `"ETH"`) |
| `spreadBps` | 8 | Spread from fair price in basis points |
| `takeProfitBps` | 0.1 | Close mode spread in basis points |
| `orderSizeUsd` | 3000 | Order size in USD |
| `closeThresholdUsd` | 10 | Position USD threshold for close mode |
| `warmupSeconds` | 10 | Seconds of price data before quoting |
| `updateThrottleMs` | 100 | Quote update throttle |
| `orderSyncIntervalMs` | 3000 | Order sync with server interval |
| `statusIntervalMs` | 1000 | Status log interval |
| `fairPriceWindowMs` | 300000 | Fair price sample window (5 min) |
| `positionSyncIntervalMs` | 5000 | Position sync with server interval |

### Environment Variables (`.env`)

| Variable | Required | Description |
|---|---|---|
| `PRIVATE_KEY` | Yes | Base58-encoded Solana wallet private key |
| `RPC_URL` | No | Solana RPC URL (for monitor) |
| `LOG_LEVEL` | No | `debug` / `info` / `warn` / `error` |

## Running

### Local

```bash
npm install
cp .env.example .env        # Set PRIVATE_KEY

npm run bot -- BTC           # Run BTC market maker
npm run bot -- ETH           # Run ETH market maker
npm run monitor -- BTC       # TUI market monitor
```

### Docker

```bash
docker compose up -d --build   # Build and start (ETH by default)
docker compose logs -f         # Follow logs
docker compose down            # Stop
```

The Dockerfile uses a multi-stage build (Node 25-slim) and runs as a non-root user.

## Key Dependencies

| Package | Purpose |
|---|---|
| `@n1xyz/nord-ts` | Official 01 Exchange SDK |
| `@solana/web3.js` | Solana blockchain client |
| `ws` | WebSocket client |
| `blessed` | Terminal UI for monitor |
| `dotenv` | Environment variable loading |
| `lodash-es` | Throttle utility |
| `bs58` | Base58 encoding/decoding |

Dev: `typescript`, `tsx`, `@biomejs/biome`

## Connection Resilience

All three WebSocket streams (Binance, 01 orderbook, account) implement the same resilience pattern:

- **Auto-reconnect**: 3-second delay after disconnect, with full state re-sync
- **Staleness detection**: 60-second threshold checked every 10 seconds; stale connections force-closed and reconnected
- **Keep-alive**: Binance feed uses ping/pong (30s interval, 10s timeout)
- **Two-phase orderbook sync**: Subscribe first, fetch snapshot, apply buffered deltas — prevents gaps
- **Order re-sync on reconnect**: Account stream fetches current orders from server to prevent desync

## Notable Design Decisions

1. **Offset-median fair price** — Robust against outliers and flash crashes compared to simple average. Circular buffer limits memory usage.

2. **Two-phase orderbook initialization** — Subscribes to deltas before fetching the snapshot, buffering updates to avoid gaps. Self-correcting on sequence breaks.

3. **Optimistic position tracking** — Updates immediately on fill events for fast strategy response, verified every 5 seconds against the server with drift warnings.

4. **Throttled quote updates (100ms)** — Reduces unnecessary order modifications and transaction costs while still responding promptly to price changes.

5. **Atomic order operations** — Cancel + place in a single on-chain transaction ensures no exposed gaps in quoting. Chunked at 4 actions per call (protocol limit).

6. **Reconnection without replay** — Fetches fresh state on reconnect rather than replaying missed messages. Simpler and avoids double-fill risks.

7. **Interface-based fair price** — `FairPriceProvider` interface enables swapping pricing strategies (e.g., EMA, VWAP) without changing the bot.

8. **Graceful shutdown** — Cancels all open orders atomically on SIGINT/SIGTERM, preventing orphaned orders.
