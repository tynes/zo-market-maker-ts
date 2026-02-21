use ed25519_dalek::{Signer, SigningKey};

use crate::error::Result;

#[cfg(feature = "solana")]
use crate::error::NordError;

/// Sign a payload by hex-encoding it first, then signing the hex string.
/// This matches the `user_sign(x) => ed25519_sign(hex(x))` scheme.
pub async fn sign_hex_encoded_payload(payload: &[u8], signing_key: &SigningKey) -> Result<Vec<u8>> {
    let hex_encoded = hex::encode(payload);
    let signature = signing_key.sign(hex_encoded.as_bytes());
    Ok(signature.to_bytes().to_vec())
}

/// Sign a payload directly (used for session-based signing).
/// This matches the `session_sign(x) => ed25519_sign(x)` scheme.
pub async fn sign_raw_payload(payload: &[u8], signing_key: &SigningKey) -> Result<Vec<u8>> {
    let signature = signing_key.sign(payload);
    Ok(signature.to_bytes().to_vec())
}

/// Sign a payload framed as a Solana transaction.
/// This matches the `admin_sign(x) => ed25519_sign(solana_frame(x))` scheme.
///
/// The Solana framing wraps the payload as a memo instruction in a
/// Solana transaction, then extracts the signature. This is used for
/// admin operations and session creation when `__use_solana_transaction_framing__`
/// is enabled.
#[cfg(feature = "solana")]
pub async fn sign_solana_transaction_framed_payload(
    payload: &[u8],
    user_pubkey: &[u8; 32],
    signing_key: &SigningKey,
) -> Result<Vec<u8>> {
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::message::Message;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::signature::Keypair;
    use solana_sdk::signer::Signer as _;
    use solana_sdk::transaction::Transaction;

    let memo_program_id = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"
        .parse::<Pubkey>()
        .map_err(|e| NordError::Signing(format!("invalid memo program id: {e}")))?;

    let user_pk = Pubkey::new_from_array(*user_pubkey);

    let instruction = Instruction {
        program_id: memo_program_id,
        accounts: vec![AccountMeta::new_readonly(user_pk, true)],
        data: payload.to_vec(),
    };

    let message = Message::new(&[instruction], Some(&user_pk));

    let keypair_bytes: [u8; 64] = {
        let mut buf = [0u8; 64];
        buf[..32].copy_from_slice(&signing_key.to_bytes());
        buf[32..].copy_from_slice(signing_key.verifying_key().as_bytes());
        buf
    };
    let keypair = Keypair::from_bytes(&keypair_bytes)
        .map_err(|e| NordError::Signing(format!("invalid keypair: {e}")))?;

    let mut tx = Transaction::new_unsigned(message);
    tx.sign(&[&keypair], solana_sdk::hash::Hash::default());

    let sig = tx
        .signatures
        .first()
        .ok_or_else(|| NordError::Signing("no signature in transaction".into()))?;

    Ok(sig.as_ref().to_vec())
}
