use std::sync::Arc;

use rust_decimal::Decimal;

use crate::actions::{create_action, send_action, SignFn};
use crate::client::Nord;
use crate::error::{NordError, Result};
use crate::proto::nord;
use crate::types::AclRole;
use crate::utils::{decode_hex, to_scaled_u64};

/// Administrative client for privileged configuration actions.
pub struct NordAdmin {
    nord: Arc<Nord>,
    admin_pubkey: [u8; 32],
    sign_fn: Box<SignFn>,
    nonce: std::sync::atomic::AtomicU32,
}

impl NordAdmin {
    /// Create a new admin client.
    pub fn new(
        nord: Arc<Nord>,
        admin_pubkey: [u8; 32],
        sign_fn: Box<SignFn>,
    ) -> Self {
        Self {
            nord,
            admin_pubkey,
            sign_fn,
            nonce: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn get_nonce(&self) -> u32 {
        self.nonce
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    fn acl_pubkey_vec(&self) -> Vec<u8> {
        self.admin_pubkey.to_vec()
    }

    async fn submit_action(
        &self,
        kind: nord::action::Kind,
    ) -> Result<nord::Receipt> {
        let timestamp = self.nord.get_timestamp().await?;
        let nonce = self.get_nonce();
        let action = create_action(timestamp, nonce, kind);
        send_action(&self.nord.http_client, &action, &self.sign_fn).await
    }

    /// Update ACL permissions for a user.
    pub async fn update_acl(
        &self,
        target: &[u8; 32],
        add_roles: &[AclRole],
        remove_roles: &[AclRole],
    ) -> Result<u64> {
        let add_mask: u32 = add_roles.iter().map(|r| r.mask()).fold(0, |a, b| a | b);
        let remove_mask: u32 = remove_roles.iter().map(|r| r.mask()).fold(0, |a, b| a | b);

        // roles_mask is the union of bits we want to change
        let roles_mask = add_mask | remove_mask;
        // roles_value is which of those bits should be set (1) vs cleared (0)
        let roles_value = add_mask;

        let kind = nord::action::Kind::UpdateAcl(nord::action::UpdateAcl {
            acl_pubkey: self.acl_pubkey_vec(),
            target_pubkey: target.to_vec(),
            roles_mask,
            roles_value,
        });

        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "update_acl")
    }

    /// Register a new token.
    pub async fn create_token(
        &self,
        token_decimals: u32,
        weight_bps: u32,
        view_symbol: &str,
        oracle_symbol: &str,
        sol_addr: &[u8; 32],
    ) -> Result<u64> {
        let kind = nord::action::Kind::CreateToken(nord::action::CreateToken {
            token_decimals,
            weight_bps,
            view_symbol: view_symbol.to_string(),
            oracle_symbol: oracle_symbol.to_string(),
            sol_addr: sol_addr.to_vec(),
            acl_pubkey: self.acl_pubkey_vec(),
        });

        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "create_token")
    }

    /// Open a new market.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_market(
        &self,
        size_decimals: u32,
        price_decimals: u32,
        imf_bps: u32,
        cmf_bps: u32,
        mmf_bps: u32,
        market_type: i32,
        view_symbol: &str,
        oracle_symbol: &str,
        base_token_id: u32,
    ) -> Result<u64> {
        let kind = nord::action::Kind::CreateMarket(nord::action::CreateMarket {
            size_decimals,
            price_decimals,
            imf_bps,
            cmf_bps,
            mmf_bps,
            market_type,
            view_symbol: view_symbol.to_string(),
            oracle_symbol: oracle_symbol.to_string(),
            base_token_id,
            acl_pubkey: self.acl_pubkey_vec(),
        });

        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "create_market")
    }

    /// Update Pyth Wormhole guardian set.
    pub async fn pyth_set_wormhole_guardians(
        &self,
        guardian_set_index: u32,
        addresses: &[String],
    ) -> Result<u64> {
        let decoded_addresses: Vec<Vec<u8>> =
            addresses.iter().map(|a| decode_hex(a)).collect();

        let kind = nord::action::Kind::PythSetWormholeGuardians(
            nord::action::PythSetWormholeGuardians {
                guardian_set_index,
                addresses: decoded_addresses,
                acl_pubkey: self.acl_pubkey_vec(),
            },
        );

        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "pyth_set_wormhole_guardians")
    }

    /// Link an oracle symbol to a Pyth price feed.
    pub async fn pyth_set_symbol_feed(
        &self,
        oracle_symbol: &str,
        price_feed_id: &str,
    ) -> Result<u64> {
        let feed_bytes = decode_hex(price_feed_id);

        let kind = nord::action::Kind::PythSetSymbolFeed(nord::action::PythSetSymbolFeed {
            oracle_symbol: oracle_symbol.to_string(),
            price_feed_id: feed_bytes,
            acl_pubkey: self.acl_pubkey_vec(),
        });

        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "pyth_set_symbol_feed")
    }

    /// Pause all trading.
    pub async fn pause(&self) -> Result<u64> {
        let kind = nord::action::Kind::Pause(nord::action::Pause {
            acl_pubkey: self.acl_pubkey_vec(),
        });
        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "pause")
    }

    /// Unpause trading.
    pub async fn unpause(&self) -> Result<u64> {
        let kind = nord::action::Kind::Unpause(nord::action::Unpause {
            acl_pubkey: self.acl_pubkey_vec(),
        });
        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "unpause")
    }

    /// Freeze a market.
    pub async fn freeze_market(&self, market_id: u32) -> Result<u64> {
        let kind = nord::action::Kind::FreezeMarket(nord::action::FreezeMarket {
            acl_pubkey: self.acl_pubkey_vec(),
            market_id,
        });
        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "freeze_market")
    }

    /// Unfreeze a market.
    pub async fn unfreeze_market(&self, market_id: u32) -> Result<u64> {
        let kind = nord::action::Kind::UnfreezeMarket(nord::action::UnfreezeMarket {
            acl_pubkey: self.acl_pubkey_vec(),
            market_id,
        });
        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "unfreeze_market")
    }

    /// Add a new fee tier.
    pub async fn add_fee_tier(
        &self,
        maker_fee_ppm: u32,
        taker_fee_ppm: u32,
    ) -> Result<u64> {
        let kind = nord::action::Kind::AddFeeTier(nord::action::AddFeeTier {
            acl_pubkey: self.acl_pubkey_vec(),
            config: Some(nord::FeeTierConfig {
                maker_fee_ppm,
                taker_fee_ppm,
            }),
        });
        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "add_fee_tier")
    }

    /// Update an existing fee tier.
    pub async fn update_fee_tier(
        &self,
        tier_id: u32,
        maker_fee_ppm: u32,
        taker_fee_ppm: u32,
    ) -> Result<u64> {
        let kind = nord::action::Kind::UpdateFeeTier(nord::action::UpdateFeeTier {
            id: tier_id,
            config: Some(nord::FeeTierConfig {
                maker_fee_ppm,
                taker_fee_ppm,
            }),
            acl_pubkey: self.acl_pubkey_vec(),
        });
        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "update_fee_tier")
    }

    /// Assign a fee tier to accounts.
    pub async fn update_accounts_tier(
        &self,
        accounts: &[u32],
        tier_id: u32,
    ) -> Result<u64> {
        let kind = nord::action::Kind::UpdateAccountsTier(nord::action::UpdateAccountsTier {
            tier_id,
            accounts: accounts.to_vec(),
            acl_pubkey: self.acl_pubkey_vec(),
        });
        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "update_accounts_tier")
    }

    /// Transfer from the fee vault.
    pub async fn fee_vault_transfer(
        &self,
        recipient: u32,
        token_id: u32,
        amount: Decimal,
    ) -> Result<u64> {
        let token = self.nord.find_token(token_id)?;
        let amount_wire = to_scaled_u64(amount, token.decimals as u32);

        let kind = nord::action::Kind::FeeVaultTransfer(nord::action::FeeVaultTransfer {
            acl_pubkey: self.acl_pubkey_vec(),
            recipient,
            token_id,
            amount: amount_wire,
        });
        let receipt = self.submit_action(kind).await?;
        Self::extract_action_id(receipt, "fee_vault_transfer")
    }

    fn extract_action_id(receipt: nord::Receipt, op: &str) -> Result<u64> {
        match receipt.kind {
            Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
                "{op} failed: error code {code}"
            ))),
            Some(_) => Ok(receipt.action_id),
            None => Err(NordError::ReceiptError(format!(
                "{op}: empty receipt"
            ))),
        }
    }
}
