use crate::error::{NordError, Result};
use crate::proto::nord;
use crate::rest::NordHttpClient;

use super::{create_action, send_action, SignFn};

/// Session TTL in microseconds (24 hours).
pub const SESSION_TTL: u64 = 24 * 60 * 60 * 1_000_000;

/// Create a new session on the exchange.
///
/// Returns `(action_id, session_id)`.
pub async fn create_session(
    http_client: &NordHttpClient,
    sign_fn: &SignFn,
    timestamp: u64,
    nonce: u32,
    user_pubkey: &[u8; 32],
    session_pubkey: &[u8; 32],
    expiry_timestamp: Option<u64>,
) -> Result<(u64, u64)> {
    let expiry = expiry_timestamp.unwrap_or(timestamp + SESSION_TTL);

    let kind = nord::action::Kind::CreateSession(nord::action::CreateSession {
        user_pubkey: user_pubkey.to_vec(),
        session_pubkey: session_pubkey.to_vec(),
        expiry_timestamp: expiry as i64,
        signature_framing: None,
    });

    let action = create_action(timestamp, nonce, kind);
    let receipt = send_action(http_client, &action, sign_fn).await?;

    match receipt.kind {
        Some(nord::receipt::Kind::CreateSessionResult(r)) => Ok((receipt.action_id, r.session_id)),
        Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
            "create session failed: error code {code}"
        ))),
        _ => Err(NordError::ReceiptError(
            "unexpected receipt for create session".into(),
        )),
    }
}

/// Revoke an existing session.
///
/// Returns the action_id.
pub async fn revoke_session(
    http_client: &NordHttpClient,
    sign_fn: &SignFn,
    timestamp: u64,
    nonce: u32,
    session_id: u64,
) -> Result<u64> {
    let kind = nord::action::Kind::RevokeSession(nord::action::RevokeSession { session_id });

    let action = create_action(timestamp, nonce, kind);
    let receipt = send_action(http_client, &action, sign_fn).await?;

    match receipt.kind {
        Some(nord::receipt::Kind::SessionRevoked(_)) => Ok(receipt.action_id),
        Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
            "revoke session failed: error code {code}"
        ))),
        _ => Err(NordError::ReceiptError(
            "unexpected receipt for revoke session".into(),
        )),
    }
}
