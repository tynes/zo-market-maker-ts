//! ZoClient factory — connects to 01 Exchange mainnet.
//!
//! Wraps the nord SDK initialisation into a single `create_zo_client` call
//! that produces a ready-to-trade [`ZoClient`].

use std::sync::Arc;

use nord::{Nord, NordConfig, NordUser};
use tracing::info;

use crate::error::ZoError;

/// A fully-initialised exchange client with authenticated user session.
pub struct ZoClient {
    /// Shared Nord HTTP/WS client.
    pub nord: Arc<Nord>,
    /// Authenticated user with active session.
    pub user: NordUser,
    /// Primary account ID on the exchange.
    pub account_id: u32,
}

/// Return the 01 Exchange mainnet configuration.
pub fn mainnet_config() -> NordConfig {
    NordConfig {
        web_server_url: "https://zo-mainnet.n1.xyz".into(),
        app: "zoau54n5U24GHNKqyoziVaVxgsiQYnPMx33fKmLLCT5".into(),
        solana_rpc_url: "https://api.mainnet-beta.solana.com".into(),
        proton_url: None,
    }
}

/// Create a fully-initialised exchange client from a bs58-encoded private key.
///
/// This:
/// 1. Connects to 01 Exchange mainnet and fetches market/token info.
/// 2. Creates a `NordUser` from the private key.
/// 3. Establishes a session and fetches account data.
///
/// # Errors
///
/// Returns [`ZoError::NoAccount`] if the wallet has no exchange account.
/// Returns [`ZoError::Nord`] for any SDK-level error.
pub async fn create_zo_client(private_key: &str) -> Result<ZoClient, ZoError> {
    info!("connecting to 01 Exchange (mainnet)");

    let config = mainnet_config();
    let nord = Arc::new(Nord::new(config).await?);

    let mut user = NordUser::from_private_key(Arc::clone(&nord), private_key)?;
    // Log truncated public key for identification.
    let pk = &user.public_key;
    info!(
        wallet = format!("{:02x}{:02x}..{:02x}{:02x}", pk[0], pk[1], pk[30], pk[31]),
        "wallet loaded"
    );

    user.refresh_session().await?;
    user.update_account_id().await?;
    user.fetch_info().await?;

    let account_id = user
        .account_ids
        .as_ref()
        .and_then(|ids| ids.first().copied())
        .ok_or(ZoError::NoAccount)?;

    info!(account_id, "connected");

    Ok(ZoClient {
        nord,
        user,
        account_id,
    })
}
