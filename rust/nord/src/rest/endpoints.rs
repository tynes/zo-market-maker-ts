use crate::error::Result;
use crate::rest::NordHttpClient;
use crate::types::*;

impl NordHttpClient {
    // --- Market Info ---

    /// GET /info - List of markets and tokens.
    pub async fn get_info(&self) -> Result<MarketsInfo> {
        self.get("/info", &[]).await
    }

    /// GET /timestamp - Current logical timestamp of the engine.
    pub async fn get_timestamp(&self) -> Result<u64> {
        self.get("/timestamp", &[]).await
    }

    /// GET /event/last-acked-nonce - Last acknowledged event nonce.
    pub async fn get_action_nonce(&self) -> Result<u64> {
        self.get("/event/last-acked-nonce", &[]).await
    }

    /// GET /action/last-executed-id - Last executed action ID.
    pub async fn get_last_action_id(&self) -> Result<u64> {
        self.get("/action/last-executed-id", &[]).await
    }

    /// GET /action?from=&to= - Query past executed actions.
    pub async fn get_actions(&self, from: u64, to: u64) -> Result<Vec<ActionsItem>> {
        self.get(
            "/action",
            &[("from", &from.to_string()), ("to", &to.to_string())],
        )
        .await
    }

    // --- User ---

    /// GET /user/{pubkey} - List account IDs and sessions for a user.
    pub async fn get_user(&self, pubkey: &str) -> Result<User> {
        self.get(&format!("/user/{pubkey}"), &[]).await
    }

    // --- Account ---

    /// GET /account/{account_id} - Account summary (balances, positions, orders).
    pub async fn get_account(&self, account_id: u32) -> Result<Account> {
        self.get(&format!("/account/{account_id}"), &[]).await
    }

    /// GET /account/{account_id}/pubkey - Account public key.
    pub async fn get_account_pubkey(&self, account_id: u32) -> Result<String> {
        self.get(&format!("/account/{account_id}/pubkey"), &[])
            .await
    }

    /// GET /account/{account_id}/fees/withdrawal - Withdrawal fee for account.
    pub async fn get_account_withdrawal_fee(&self, account_id: u32) -> Result<f64> {
        self.get(&format!("/account/{account_id}/fees/withdrawal"), &[])
            .await
    }

    /// GET /account/{account_id}/orders - Open orders with pagination.
    pub async fn get_account_orders(
        &self,
        account_id: u32,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<OrderInfo>> {
        let mut query = Vec::new();
        let si_str;
        let ps_str;
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get(&format!("/account/{account_id}/orders"), &query)
            .await
    }

    /// GET /account/{account_id}/history/pnl - PnL history.
    pub async fn get_account_pnl(
        &self,
        account_id: u32,
        market_id: Option<u32>,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<AccountPnlInfo>> {
        let mut query = Vec::new();
        let mid_str;
        let si_str;
        let ps_str;
        if let Some(mid) = market_id {
            mid_str = mid.to_string();
            query.push(("marketId", mid_str.as_str()));
        }
        if let Some(s) = since {
            query.push(("since", s));
        }
        if let Some(u) = until {
            query.push(("until", u));
        }
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get(&format!("/account/{account_id}/history/pnl"), &query)
            .await
    }

    /// GET /account/{account_id}/history/funding - Funding payment history.
    pub async fn get_account_funding_history(
        &self,
        account_id: u32,
        market_id: Option<u32>,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<&str>,
        page_size: Option<u8>,
    ) -> Result<PageResult<AccountFundingInfo>> {
        let mut query = Vec::new();
        let mid_str;
        let ps_str;
        if let Some(mid) = market_id {
            mid_str = mid.to_string();
            query.push(("marketId", mid_str.as_str()));
        }
        if let Some(s) = since {
            query.push(("since", s));
        }
        if let Some(u) = until {
            query.push(("until", u));
        }
        if let Some(si) = start_inclusive {
            query.push(("startInclusive", si));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get(&format!("/account/{account_id}/history/funding"), &query)
            .await
    }

    /// GET /account/{account_id}/triggers - Active triggers.
    pub async fn get_account_triggers(&self, account_id: u32) -> Result<Option<Vec<TriggerInfo>>> {
        self.get(&format!("/account/{account_id}/triggers"), &[])
            .await
    }

    /// GET /account/{account_id}/triggers/history - Trigger history.
    pub async fn get_account_trigger_history(
        &self,
        account_id: u32,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<Trigger>> {
        let mut query = Vec::new();
        let si_str;
        let ps_str;
        if let Some(s) = since {
            query.push(("since", s));
        }
        if let Some(u) = until {
            query.push(("until", u));
        }
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get(&format!("/account/{account_id}/triggers/history"), &query)
            .await
    }

    /// GET /account/{account_id}/history/withdrawal - Withdrawal history.
    pub async fn get_account_withdrawal_history(
        &self,
        account_id: u32,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<WithdrawalInfo>> {
        let mut query = Vec::new();
        let si_str;
        let ps_str;
        if let Some(s) = since {
            query.push(("since", s));
        }
        if let Some(u) = until {
            query.push(("until", u));
        }
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get(&format!("/account/{account_id}/history/withdrawal"), &query)
            .await
    }

    /// GET /account/{account_id}/history/deposit - Deposit history.
    pub async fn get_account_deposit_history(
        &self,
        account_id: u32,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<DepositInfo>> {
        let mut query = Vec::new();
        let si_str;
        let ps_str;
        if let Some(s) = since {
            query.push(("since", s));
        }
        if let Some(u) = until {
            query.push(("until", u));
        }
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get(&format!("/account/{account_id}/history/deposit"), &query)
            .await
    }

    /// GET /account/{account_id}/history/liquidation - Liquidation history.
    pub async fn get_account_liquidation_history(
        &self,
        account_id: u32,
        since: Option<&str>,
        until: Option<&str>,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<LiquidationInfo>> {
        let mut query = Vec::new();
        let si_str;
        let ps_str;
        if let Some(s) = since {
            query.push(("since", s));
        }
        if let Some(u) = until {
            query.push(("until", u));
        }
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get(
            &format!("/account/{account_id}/history/liquidation"),
            &query,
        )
        .await
    }

    /// GET /account/{account_id}/fee/tier - Fee tier for an account.
    pub async fn get_account_fee_tier(&self, account_id: u32) -> Result<FeeTierId> {
        self.get(&format!("/account/{account_id}/fee/tier"), &[])
            .await
    }

    /// GET /account/volume - Trading volume for an account.
    pub async fn get_account_volume(
        &self,
        account_id: u32,
        market_id: Option<u32>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<AccountVolumeInfo>> {
        let mut query = vec![("accountId", account_id.to_string())];
        if let Some(mid) = market_id {
            query.push(("marketId", mid.to_string()));
        }
        if let Some(s) = since {
            query.push(("since", s.to_string()));
        }
        if let Some(u) = until {
            query.push(("until", u.to_string()));
        }
        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.get("/account/volume", &query_refs).await
    }

    // --- Market ---

    /// GET /market/{market_id}/orderbook - Orderbook for a market.
    pub async fn get_orderbook(&self, market_id: u32) -> Result<OrderbookInfo> {
        self.get(&format!("/market/{market_id}/orderbook"), &[])
            .await
    }

    /// GET /market/{market_id}/stats - Market statistics.
    pub async fn get_market_stats(&self, market_id: u32) -> Result<MarketStats> {
        self.get(&format!("/market/{market_id}/stats"), &[]).await
    }

    /// GET /market/{market_id}/fees/{fee_kind}/{account_id} - Fee for an account on a market.
    pub async fn get_market_fee(
        &self,
        market_id: u32,
        fee_kind: &FillRole,
        account_id: u32,
    ) -> Result<f64> {
        self.get(
            &format!("/market/{market_id}/fees/{fee_kind}/{account_id}"),
            &[],
        )
        .await
    }

    // --- Token ---

    /// GET /tokens/{token_id}/stats - Token statistics.
    pub async fn get_token_stats(&self, token_id: u32) -> Result<TokenStats> {
        self.get(&format!("/tokens/{token_id}/stats"), &[]).await
    }

    // --- Order ---

    /// GET /order/{order_id} - Order summary.
    pub async fn get_order(&self, order_id: u64) -> Result<OrderInfo> {
        self.get(&format!("/order/{order_id}"), &[]).await
    }

    /// GET /order/{order_id}/trades - Trades for an order.
    pub async fn get_order_trades(
        &self,
        order_id: u64,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<Trade>> {
        let mut query = Vec::new();
        let si_str;
        let ps_str;
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get(&format!("/order/{order_id}/trades"), &query).await
    }

    /// GET /trades - Trade history with filters.
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
        let mut query = Vec::new();
        let mid_str;
        let tid_str;
        let mkid_str;
        let si_str;
        let ps_str;
        if let Some(mid) = market_id {
            mid_str = mid.to_string();
            query.push(("marketId", mid_str.as_str()));
        }
        if let Some(tid) = taker_id {
            tid_str = tid.to_string();
            query.push(("takerId", tid_str.as_str()));
        }
        if let Some(mkid) = maker_id {
            mkid_str = mkid.to_string();
            query.push(("makerId", mkid_str.as_str()));
        }
        if let Some(ts) = taker_side {
            query.push(("takerSide", ts));
        }
        if let Some(s) = since {
            query.push(("since", s));
        }
        if let Some(u) = until {
            query.push(("until", u));
        }
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get("/trades", &query).await
    }

    // --- Fee ---

    /// GET /fee/brackets/info - Fee tier brackets.
    pub async fn get_fee_brackets(&self) -> Result<Vec<(FeeTierId, FeeTierConfig)>> {
        self.get("/fee/brackets/info", &[]).await
    }

    /// GET /accounts/fee-tiers - List account fee tiers.
    pub async fn get_accounts_fee_tiers(
        &self,
        tier: Option<FeeTierId>,
        start_inclusive: Option<u32>,
        page_size: Option<u8>,
    ) -> Result<PageResult<AccountFeeTier>> {
        let mut query = Vec::new();
        let t_str;
        let si_str;
        let ps_str;
        if let Some(t) = tier {
            t_str = t.to_string();
            query.push(("tier", t_str.as_str()));
        }
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get("/accounts/fee-tiers", &query).await
    }

    // --- Admin ---

    /// GET /admin - List of assigned admins.
    pub async fn get_admin_list(&self) -> Result<Vec<AdminInfo>> {
        self.get("/admin", &[]).await
    }

    // --- Triggers ---

    /// GET /triggers/active - All active triggers (paginated).
    pub async fn get_active_triggers(
        &self,
        start_inclusive: Option<u64>,
        page_size: Option<u8>,
    ) -> Result<PageResult<TriggerInfo>> {
        let mut query = Vec::new();
        let si_str;
        let ps_str;
        if let Some(si) = start_inclusive {
            si_str = si.to_string();
            query.push(("startInclusive", si_str.as_str()));
        }
        if let Some(ps) = page_size {
            ps_str = ps.to_string();
            query.push(("pageSize", ps_str.as_str()));
        }
        self.get("/triggers/active", &query).await
    }

    // --- Accounts count ---

    /// GET /accounts/count - Total number of accounts.
    pub async fn get_accounts_count(&self) -> Result<u64> {
        self.get("/accounts/count", &[]).await
    }
}
