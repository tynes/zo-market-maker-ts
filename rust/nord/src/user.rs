use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use rust_decimal::Decimal;

use crate::actions::atomic::{AtomicSubaction, UserAtomicSubaction};
use crate::actions::session::{create_session, revoke_session, SESSION_TTL};
use crate::actions::signing::{sign_hex_encoded_payload, sign_raw_payload};
use crate::actions::{create_action, send_action, SignFn};
use crate::client::Nord;
use crate::error::{NordError, Result};
use crate::proto::nord;
use crate::types::*;
use crate::utils::{to_scaled_u128, to_scaled_u64};

/// User client for the Nord exchange.
///
/// Manages session-based authentication, trading, and account queries.
pub struct NordUser {
    pub nord: Arc<Nord>,
    pub public_key: [u8; 32],
    pub session_pubkey: [u8; 32],
    pub session_id: Option<u64>,
    nonce: AtomicU32,
    /// Signing function for user-level actions (hex-encoded).
    sign_user_fn: Box<SignFn>,
    /// Signing function for session-level actions (raw).
    sign_session_fn: Box<SignFn>,

    pub account_ids: Option<Vec<u32>>,
    pub balances: HashMap<String, Vec<UserBalance>>,
    pub orders: HashMap<String, Vec<OpenOrder>>,
    pub positions: HashMap<String, Vec<PositionSummary>>,
    pub margins: HashMap<String, AccountMarginsView>,
    pub spl_token_infos: Vec<SPLTokenInfo>,
}

/// A user's token balance within a specific account.
#[derive(Debug, Clone)]
pub struct UserBalance {
    pub account_id: u32,
    pub balance: f64,
    pub symbol: String,
}

impl NordUser {
    /// Create a NordUser from a private key string (bs58 encoded).
    pub fn from_private_key(nord: Arc<Nord>, private_key: &str) -> Result<Self> {
        let signing_key = crate::utils::keypair_from_private_key(private_key)?;
        let public_key = signing_key.verifying_key().to_bytes();

        // For session, generate a separate keypair.
        let session_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let session_pubkey = session_key.verifying_key().to_bytes();

        let user_key = signing_key.clone();
        let sign_user_fn: Box<SignFn> = Box::new(move |payload: &[u8]| {
            let key = user_key.clone();
            let payload = payload.to_vec();
            Box::pin(async move { sign_hex_encoded_payload(&payload, &key).await })
        });

        let sess_key = session_key.clone();
        let sign_session_fn: Box<SignFn> = Box::new(move |payload: &[u8]| {
            let key = sess_key.clone();
            let payload = payload.to_vec();
            Box::pin(async move { sign_raw_payload(&payload, &key).await })
        });

        let spl_token_infos: Vec<SPLTokenInfo> = nord
            .tokens
            .iter()
            .map(|t| SPLTokenInfo {
                mint: t.mint_addr.clone(),
                precision: t.decimals,
                token_id: t.token_id,
                name: t.symbol.clone(),
            })
            .collect();

        Ok(Self {
            nord,
            public_key,
            session_pubkey,
            session_id: None,
            nonce: AtomicU32::new(0),
            sign_user_fn,
            sign_session_fn,
            account_ids: None,
            balances: HashMap::new(),
            orders: HashMap::new(),
            positions: HashMap::new(),
            margins: HashMap::new(),
            spl_token_infos,
        })
    }

    /// Get a new nonce for actions.
    pub fn get_nonce(&self) -> u32 {
        self.nonce.fetch_add(1, Ordering::SeqCst)
    }

    /// Refresh the session (create a new one).
    pub async fn refresh_session(&mut self) -> Result<()> {
        let timestamp = self.nord.get_timestamp().await?;
        let nonce = self.get_nonce();

        let (action_id, session_id) = create_session(
            &self.nord.http_client,
            &self.sign_user_fn,
            timestamp,
            nonce,
            &self.public_key,
            &self.session_pubkey,
            Some(timestamp + SESSION_TTL),
        )
        .await?;

        tracing::info!(action_id, session_id, "session created");
        self.session_id = Some(session_id);
        Ok(())
    }

    /// Revoke a session.
    pub async fn revoke_session(&mut self, session_id: u64) -> Result<()> {
        let timestamp = self.nord.get_timestamp().await?;
        let nonce = self.get_nonce();

        revoke_session(
            &self.nord.http_client,
            &self.sign_user_fn,
            timestamp,
            nonce,
            session_id,
        )
        .await?;

        if self.session_id == Some(session_id) {
            self.session_id = None;
        }
        Ok(())
    }

    /// Update account IDs by querying the server.
    pub async fn update_account_id(&mut self) -> Result<()> {
        let pubkey = bs58::encode(&self.public_key).into_string();
        let user = self.nord.get_user(&pubkey).await?;
        self.account_ids = Some(user.account_ids);
        Ok(())
    }

    /// Fetch account info (balances, orders, positions, margins).
    pub async fn fetch_info(&mut self) -> Result<()> {
        let account_ids = self
            .account_ids
            .as_ref()
            .ok_or(NordError::NoAccount)?
            .clone();

        self.balances.clear();
        self.orders.clear();
        self.positions.clear();
        self.margins.clear();

        for &account_id in &account_ids {
            let account = self.nord.get_account(account_id).await?;
            let key = account_id.to_string();

            let balances: Vec<UserBalance> = account
                .balances
                .iter()
                .map(|b| UserBalance {
                    account_id,
                    balance: b.amount,
                    symbol: b.token.clone(),
                })
                .collect();

            self.balances.insert(key.clone(), balances);
            self.orders.insert(key.clone(), account.orders);
            self.positions.insert(key.clone(), account.positions);
            self.margins.insert(key.clone(), account.margins);
        }

        Ok(())
    }

    fn check_session(&self) -> Result<u64> {
        self.session_id
            .ok_or_else(|| NordError::SessionInvalid("no active session".into()))
    }

    fn default_account_id(&self) -> Result<u32> {
        self.account_ids
            .as_ref()
            .and_then(|ids| ids.first().copied())
            .ok_or(NordError::NoAccount)
    }

    /// Submit a session-signed action.
    async fn submit_session_action(&self, kind: nord::action::Kind) -> Result<nord::Receipt> {
        let _ = self.check_session()?;
        let timestamp = self.nord.get_timestamp().await?;
        let nonce = self.get_nonce();
        let action = create_action(timestamp, nonce, kind);
        send_action(&self.nord.http_client, &action, &self.sign_session_fn).await
    }

    /// Place an order.
    #[allow(clippy::too_many_arguments)]
    pub async fn place_order(
        &self,
        market_id: u32,
        side: Side,
        fill_mode: FillMode,
        is_reduce_only: bool,
        size: Option<Decimal>,
        price: Option<Decimal>,
        quote_size: Option<QuoteSize>,
        account_id: Option<u32>,
        client_order_id: Option<u64>,
    ) -> Result<PlaceOrderResult> {
        let session_id = self.check_session()?;
        let acct = account_id.or_else(|| self.default_account_id().ok());

        let market = self.nord.find_market(market_id)?;

        let (wire_price, wire_size) = if let Some(qs) = &quote_size {
            qs.to_wire(market.price_decimals as u32, market.size_decimals as u32)?
        } else {
            let p = price
                .ok_or_else(|| NordError::Validation("price or quote_size required".into()))?;
            let s =
                size.ok_or_else(|| NordError::Validation("size or quote_size required".into()))?;
            (
                to_scaled_u64(p, market.price_decimals as u32)?,
                to_scaled_u64(s, market.size_decimals as u32)?,
            )
        };

        let proto_side: i32 = match side {
            Side::Ask => nord::Side::Ask as i32,
            Side::Bid => nord::Side::Bid as i32,
        };

        let proto_quote_size = quote_size
            .as_ref()
            .map(|qs| {
                let val = to_scaled_u128(
                    qs.value(),
                    market.price_decimals as u32 + market.size_decimals as u32,
                )?;
                Ok::<_, NordError>(nord::U128 {
                    lo: val as u64,
                    hi: (val >> 64) as u64,
                })
            })
            .transpose()?;

        let kind = nord::action::Kind::PlaceOrder(nord::action::PlaceOrder {
            session_id,
            market_id,
            side: proto_side,
            fill_mode: fill_mode.to_proto() as i32,
            is_reduce_only,
            price: wire_price,
            size: wire_size,
            quote_size: proto_quote_size,
            delegator_account_id: None,
            client_order_id,
            sender_account_id: acct,
            sender_tracking_id: None,
        });

        let receipt = self.submit_session_action(kind).await?;

        match receipt.kind {
            Some(nord::receipt::Kind::PlaceOrderResult(r)) => Ok(PlaceOrderResult {
                action_id: receipt.action_id,
                order_id: r.posted.as_ref().map(|p| p.order_id),
                fills: r.fills,
            }),
            Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
                "place order failed: error code {code}"
            ))),
            _ => Err(NordError::ReceiptError(
                "unexpected receipt for place order".into(),
            )),
        }
    }

    /// Cancel an order by order ID.
    pub async fn cancel_order(
        &self,
        order_id: u64,
        account_id: Option<u32>,
    ) -> Result<CancelOrderResult> {
        let session_id = self.check_session()?;
        let acct = account_id.or_else(|| self.default_account_id().ok());

        let kind = nord::action::Kind::CancelOrderById(nord::action::CancelOrderById {
            session_id,
            order_id,
            delegator_account_id: None,
            sender_account_id: acct,
        });

        let receipt = self.submit_session_action(kind).await?;

        match receipt.kind {
            Some(nord::receipt::Kind::CancelOrderResult(r)) => Ok(CancelOrderResult {
                action_id: receipt.action_id,
                order_id: r.order_id,
                account_id: r.account_id,
            }),
            Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
                "cancel order failed: error code {code}"
            ))),
            _ => Err(NordError::ReceiptError(
                "unexpected receipt for cancel order".into(),
            )),
        }
    }

    /// Cancel an order by client order ID.
    pub async fn cancel_order_by_client_id(
        &self,
        client_order_id: u64,
        account_id: Option<u32>,
    ) -> Result<CancelOrderResult> {
        let session_id = self.check_session()?;
        let acct = account_id.or_else(|| self.default_account_id().ok());

        let kind = nord::action::Kind::CancelOrderByClientId(nord::action::CancelOrderByClientId {
            session_id,
            client_order_id,
            sender_account_id: acct,
        });

        let receipt = self.submit_session_action(kind).await?;

        match receipt.kind {
            Some(nord::receipt::Kind::CancelOrderResult(r)) => Ok(CancelOrderResult {
                action_id: receipt.action_id,
                order_id: r.order_id,
                account_id: r.account_id,
            }),
            Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
                "cancel order by client id failed: error code {code}"
            ))),
            _ => Err(NordError::ReceiptError(
                "unexpected receipt for cancel by client id".into(),
            )),
        }
    }

    /// Execute up to 4 place/cancel operations atomically.
    pub async fn atomic(
        &self,
        user_actions: &[UserAtomicSubaction],
        account_id: Option<u32>,
    ) -> Result<AtomicResult> {
        let session_id = self.check_session()?;
        let acct = account_id
            .or_else(|| self.default_account_id().ok())
            .ok_or(NordError::NoAccount)?;

        let actions: Vec<AtomicSubaction> = user_actions
            .iter()
            .map(|ua| match ua {
                UserAtomicSubaction::Place {
                    market_id,
                    side,
                    fill_mode,
                    is_reduce_only,
                    size,
                    price,
                    quote_size,
                    client_order_id,
                } => {
                    let market = self.nord.find_market(*market_id)?;
                    Ok(AtomicSubaction::Place {
                        market_id: *market_id,
                        side: *side,
                        fill_mode: *fill_mode,
                        is_reduce_only: *is_reduce_only,
                        size_decimals: market.size_decimals,
                        price_decimals: market.price_decimals,
                        size: *size,
                        price: *price,
                        quote_size: quote_size.clone(),
                        client_order_id: *client_order_id,
                    })
                }
                UserAtomicSubaction::Cancel { order_id } => Ok(AtomicSubaction::Cancel {
                    order_id: *order_id,
                }),
            })
            .collect::<Result<Vec<_>>>()?;

        let timestamp = self.nord.get_timestamp().await?;
        let nonce = self.get_nonce();

        let (action_id, results) = crate::actions::atomic::atomic(
            &self.nord.http_client,
            &self.sign_session_fn,
            timestamp,
            nonce,
            session_id,
            acct,
            &actions,
        )
        .await?;

        Ok(AtomicResult { action_id, results })
    }

    /// Add a trigger (stop-loss or take-profit).
    pub async fn add_trigger(
        &self,
        market_id: u32,
        side: Side,
        kind: TriggerKind,
        trigger_price: Decimal,
        limit_price: Option<Decimal>,
        account_id: Option<u32>,
    ) -> Result<u64> {
        let session_id = self.check_session()?;
        let acct = account_id.or_else(|| self.default_account_id().ok());

        let market = self.nord.find_market(market_id)?;
        let trigger_price_wire = to_scaled_u64(trigger_price, market.price_decimals as u32)?;
        let limit_price_wire = limit_price
            .map(|lp| to_scaled_u64(lp, market.price_decimals as u32))
            .transpose()?;

        let proto_side: i32 = match side {
            Side::Ask => nord::Side::Ask as i32,
            Side::Bid => nord::Side::Bid as i32,
        };

        let action_kind = nord::action::Kind::AddTrigger(nord::action::AddTrigger {
            session_id,
            market_id,
            key: Some(nord::TriggerKey {
                kind: kind.to_proto() as i32,
                side: proto_side,
            }),
            prices: Some(nord::action::TriggerPrices {
                trigger_price: trigger_price_wire,
                limit_price: limit_price_wire,
            }),
            account_id: acct,
        });

        let receipt = self.submit_session_action(action_kind).await?;

        match receipt.kind {
            Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
                "add trigger failed: error code {code}"
            ))),
            Some(_) => Ok(receipt.action_id),
            None => Err(NordError::ReceiptError("empty receipt".into())),
        }
    }

    /// Remove a trigger.
    pub async fn remove_trigger(
        &self,
        market_id: u32,
        side: Side,
        kind: TriggerKind,
        account_id: Option<u32>,
    ) -> Result<u64> {
        let session_id = self.check_session()?;
        let acct = account_id.or_else(|| self.default_account_id().ok());

        let proto_side: i32 = match side {
            Side::Ask => nord::Side::Ask as i32,
            Side::Bid => nord::Side::Bid as i32,
        };

        let action_kind = nord::action::Kind::RemoveTrigger(nord::action::RemoveTrigger {
            session_id,
            market_id,
            key: Some(nord::TriggerKey {
                kind: kind.to_proto() as i32,
                side: proto_side,
            }),
            account_id: acct,
        });

        let receipt = self.submit_session_action(action_kind).await?;

        match receipt.kind {
            Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
                "remove trigger failed: error code {code}"
            ))),
            Some(_) => Ok(receipt.action_id),
            None => Err(NordError::ReceiptError("empty receipt".into())),
        }
    }

    /// Transfer tokens between accounts.
    pub async fn transfer_to_account(
        &self,
        token_id: u32,
        amount: Decimal,
        from_account_id: Option<u32>,
        to_account_id: Option<u32>,
    ) -> Result<TransferResult> {
        let session_id = self.check_session()?;
        let from = from_account_id
            .or_else(|| self.default_account_id().ok())
            .ok_or(NordError::NoAccount)?;

        let token = self.nord.find_token(token_id)?;
        let amount_wire = to_scaled_u64(amount, token.decimals as u32)?;

        let recipient = to_account_id.map(|id| nord::action::Recipient {
            recipient_type: Some(nord::action::recipient::RecipientType::Owned(
                nord::action::recipient::Owned { account_id: id },
            )),
        });

        let kind = nord::action::Kind::Transfer(nord::action::Transfer {
            session_id,
            from_account_id: from,
            token_id,
            amount: amount_wire,
            to_account_id: recipient,
        });

        let receipt = self.submit_session_action(kind).await?;

        match receipt.kind {
            Some(nord::receipt::Kind::Transferred(r)) => Ok(TransferResult {
                action_id: receipt.action_id,
                account_created: r.account_created,
            }),
            Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
                "transfer failed: error code {code}"
            ))),
            _ => Err(NordError::ReceiptError(
                "unexpected receipt for transfer".into(),
            )),
        }
    }

    /// Withdraw tokens from the exchange.
    pub async fn withdraw(
        &self,
        token_id: u32,
        amount: Decimal,
        dest_pubkey: Option<&str>,
    ) -> Result<u64> {
        let session_id = self.check_session()?;
        let token = self.nord.find_token(token_id)?;
        let amount_wire = to_scaled_u64(amount, token.decimals as u32)?;

        let dest = dest_pubkey
            .filter(|s| !s.is_empty())
            .map(|s| {
                bs58::decode(s)
                    .into_vec()
                    .map_err(|e| NordError::Validation(format!("invalid dest pubkey: {e}")))
            })
            .transpose()?;

        let kind = nord::action::Kind::Withdraw(nord::action::Withdraw {
            token_id,
            session_id,
            amount: amount_wire,
            dest_pubkey: dest,
        });

        let receipt = self.submit_session_action(kind).await?;

        match receipt.kind {
            Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
                "withdraw failed: error code {code}"
            ))),
            Some(_) => Ok(receipt.action_id),
            None => Err(NordError::ReceiptError("empty receipt".into())),
        }
    }
}

// Result types for user operations.

/// Result of a place-order action.
#[derive(Debug)]
pub struct PlaceOrderResult {
    pub action_id: u64,
    pub order_id: Option<u64>,
    pub fills: Vec<nord::receipt::Trade>,
}

/// Result of a cancel-order action.
#[derive(Debug)]
pub struct CancelOrderResult {
    pub action_id: u64,
    pub order_id: u64,
    pub account_id: u32,
}

/// Result of an atomic (batched) action.
#[derive(Debug)]
pub struct AtomicResult {
    pub action_id: u64,
    pub results: Vec<nord::receipt::atomic_subaction_result_kind::Inner>,
}

/// Result of a token transfer action.
#[derive(Debug)]
pub struct TransferResult {
    pub action_id: u64,
    pub account_created: bool,
}
