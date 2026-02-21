use std::collections::HashMap;

use crate::config::NordConfig;
use crate::error::{NordError, Result};
use crate::rest::NordHttpClient;
use crate::types::*;
use crate::ws::NordWebSocketClient;

/// Main Nord client for interacting with the exchange.
#[derive(Debug, Clone)]
pub struct Nord {
    /// Base URL for the Nord web server.
    pub web_server_url: String,
    /// Solana RPC URL.
    pub solana_rpc_url: String,
    /// App address.
    pub app: String,
    /// HTTP client.
    pub http_client: NordHttpClient,
    /// Available markets.
    pub markets: Vec<MarketInfo>,
    /// Available tokens.
    pub tokens: Vec<TokenInfo>,
    /// Symbol -> market_id mapping.
    symbol_to_market_id: HashMap<String, u32>,
}

impl Nord {
    /// Create and initialize a new Nord client.
    pub async fn new(config: NordConfig) -> Result<Self> {
        let http_client = NordHttpClient::new(&config.web_server_url);

        let info = http_client.get_info().await?;

        let mut symbol_to_market_id = HashMap::new();
        for market in &info.markets {
            symbol_to_market_id.insert(market.symbol.clone(), market.market_id);
        }

        Ok(Self {
            web_server_url: config.web_server_url,
            solana_rpc_url: config.solana_rpc_url,
            app: config.app,
            http_client,
            markets: info.markets,
            tokens: info.tokens,
            symbol_to_market_id,
        })
    }

    /// Refresh market/token info from the server.
    pub async fn fetch_info(&mut self) -> Result<()> {
        let info = self.http_client.get_info().await?;
        self.symbol_to_market_id.clear();
        for market in &info.markets {
            self.symbol_to_market_id
                .insert(market.symbol.clone(), market.market_id);
        }
        self.markets = info.markets;
        self.tokens = info.tokens;
        Ok(())
    }

    /// Resolve a market symbol to its ID.
    pub fn resolve_market_id(&self, symbol: &str) -> Result<u32> {
        self.symbol_to_market_id
            .get(symbol)
            .copied()
            .ok_or_else(|| NordError::MarketNotFound(0))
    }

    /// Find a market by ID.
    pub fn find_market(&self, market_id: u32) -> Result<&MarketInfo> {
        crate::utils::find_market(&self.markets, market_id)
    }

    /// Find a token by ID.
    pub fn find_token(&self, token_id: u32) -> Result<&TokenInfo> {
        crate::utils::find_token(&self.tokens, token_id)
    }

    // --- REST delegates ---

    /// Get the current server timestamp.
    pub async fn get_timestamp(&self) -> Result<u64> {
        self.http_client.get_timestamp().await
    }

    /// Get the next action nonce.
    pub async fn get_action_nonce(&self) -> Result<u64> {
        self.http_client.get_action_nonce().await
    }

    /// Get the ID of the most recent action.
    pub async fn get_last_action_id(&self) -> Result<u64> {
        self.http_client.get_last_action_id().await
    }

    /// Get actions in the given ID range.
    pub async fn get_actions(&self, from: u64, to: u64) -> Result<Vec<ActionsItem>> {
        self.http_client.get_actions(from, to).await
    }

    /// Get user information by public key.
    pub async fn get_user(&self, pubkey: &str) -> Result<User> {
        self.http_client.get_user(pubkey).await
    }

    /// Get full account state by account ID.
    pub async fn get_account(&self, account_id: u32) -> Result<Account> {
        self.http_client.get_account(account_id).await
    }

    /// Get the public key associated with an account.
    pub async fn get_account_pubkey(&self, account_id: u32) -> Result<String> {
        self.http_client.get_account_pubkey(account_id).await
    }

    /// Get the withdrawal fee for an account.
    pub async fn get_account_withdrawal_fee(&self, account_id: u32) -> Result<f64> {
        self.http_client
            .get_account_withdrawal_fee(account_id)
            .await
    }

    /// Get paginated order history for an account.
    pub async fn get_account_orders(
        &self,
        account_id: u32,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<OrderInfo>> {
        self.http_client
            .get_account_orders(account_id, start_inclusive, page_size)
            .await
    }

    /// Get paginated PnL history for an account.
    pub async fn get_account_pnl(
        &self,
        account_id: u32,
        market_id: Option<u32>,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<AccountPnlInfo>> {
        self.http_client
            .get_account_pnl(
                account_id,
                market_id,
                since,
                until,
                start_inclusive,
                page_size,
            )
            .await
    }

    /// Get active triggers for an account.
    pub async fn get_account_triggers(&self, account_id: u32) -> Result<Option<Vec<TriggerInfo>>> {
        self.http_client.get_account_triggers(account_id).await
    }

    /// Get paginated trigger history for an account.
    pub async fn get_account_trigger_history(
        &self,
        account_id: u32,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<Trigger>> {
        self.http_client
            .get_account_trigger_history(account_id, since, until, start_inclusive, page_size)
            .await
    }

    /// Get paginated withdrawal history for an account.
    pub async fn get_account_withdrawal_history(
        &self,
        account_id: u32,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<WithdrawalInfo>> {
        self.http_client
            .get_account_withdrawal_history(account_id, since, until, start_inclusive, page_size)
            .await
    }

    /// Get the orderbook for a market by symbol name.
    pub async fn get_orderbook_by_symbol(&self, symbol: &str) -> Result<OrderbookInfo> {
        let market_id = self.resolve_market_id(symbol)?;
        self.http_client.get_orderbook(market_id).await
    }

    /// Get the orderbook for a market by ID.
    pub async fn get_orderbook(&self, market_id: u32) -> Result<OrderbookInfo> {
        self.http_client.get_orderbook(market_id).await
    }

    /// Get exchange-wide markets and tokens configuration.
    pub async fn get_info(&self) -> Result<MarketsInfo> {
        self.http_client.get_info().await
    }

    /// Get statistics for a specific market.
    pub async fn get_market_stats(&self, market_id: u32) -> Result<MarketStats> {
        self.http_client.get_market_stats(market_id).await
    }

    /// Get the fee rate for a market, role, and account combination.
    pub async fn get_market_fee(
        &self,
        market_id: u32,
        fee_kind: &FillRole,
        account_id: u32,
    ) -> Result<f64> {
        self.http_client
            .get_market_fee(market_id, fee_kind, account_id)
            .await
    }

    /// Get statistics for a specific token.
    pub async fn get_token_stats(&self, token_id: u32) -> Result<TokenStats> {
        self.http_client.get_token_stats(token_id).await
    }

    /// Get detailed information about a specific order.
    pub async fn get_order(&self, order_id: u64) -> Result<OrderInfo> {
        self.http_client.get_order(order_id).await
    }

    /// Get paginated trades for a specific order.
    pub async fn get_order_trades(
        &self,
        order_id: u64,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<Trade>> {
        self.http_client
            .get_order_trades(order_id, start_inclusive, page_size)
            .await
    }

    /// Get paginated trades with optional filters.
    #[allow(clippy::too_many_arguments)]
    pub async fn get_trades(
        &self,
        market_id: Option<u32>,
        taker_id: Option<u32>,
        maker_id: Option<u32>,
        taker_side: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<Trade>> {
        self.http_client
            .get_trades(
                market_id,
                taker_id,
                maker_id,
                taker_side,
                since,
                until,
                start_inclusive,
                page_size,
            )
            .await
    }

    /// Get all fee tier brackets.
    pub async fn get_fee_brackets(&self) -> Result<Vec<(FeeTierId, FeeTierConfig)>> {
        self.http_client.get_fee_brackets().await
    }

    /// Get the fee tier assigned to an account.
    pub async fn get_account_fee_tier(&self, account_id: u32) -> Result<FeeTierId> {
        self.http_client.get_account_fee_tier(account_id).await
    }

    /// Get paginated account-to-fee-tier mappings.
    pub async fn get_accounts_fee_tiers(
        &self,
        tier: Option<FeeTierId>,
        start_inclusive: Option<u32>,
        page_size: Option<u8>,
    ) -> Result<PageResult<AccountFeeTier>> {
        self.http_client
            .get_accounts_fee_tiers(tier, start_inclusive, page_size)
            .await
    }

    /// Get the list of admin users and their roles.
    pub async fn get_admin_list(&self) -> Result<Vec<AdminInfo>> {
        self.http_client.get_admin_list().await
    }

    /// Get trading volume for an account with optional filters.
    pub async fn get_account_volume(
        &self,
        account_id: u32,
        market_id: Option<u32>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<AccountVolumeInfo>> {
        self.http_client
            .get_account_volume(account_id, market_id, since, until)
            .await
    }

    // --- WebSocket ---

    /// Create a WebSocket client with the given subscriptions.
    pub fn create_websocket_client(
        &self,
        trades: &[String],
        deltas: &[String],
        accounts: &[u32],
        candles: &[(String, CandleResolution)],
    ) -> NordWebSocketClient {
        let mut streams = Vec::new();

        for symbol in trades {
            streams.push(format!("trades@{symbol}"));
        }
        for symbol in deltas {
            streams.push(format!("deltas@{symbol}"));
        }
        for account_id in accounts {
            streams.push(format!("account@{account_id}"));
        }
        for (symbol, resolution) in candles {
            streams.push(format!("candle@{symbol}:{resolution}"));
        }

        let ws_url = format!(
            "{}/ws/{}",
            self.web_server_url
                .replace("https://", "wss://")
                .replace("http://", "ws://"),
            streams.join("&")
        );

        NordWebSocketClient::new(ws_url)
    }
}
