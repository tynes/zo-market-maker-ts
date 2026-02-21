use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::get_associated_token_address;

use crate::error::{NordError, Result};

/// Get the associated token account for a given owner and mint.
pub fn get_ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    get_associated_token_address(owner, mint)
}

/// Deposit result containing the transaction signature and buffer account.
#[derive(Debug)]
pub struct DepositResult {
    pub signature: String,
    pub buffer: Pubkey,
}

/// Deposit SPL tokens to the exchange app.
///
/// Builds an SPL transfer transaction, signs it, and sends via RPC.
pub async fn deposit(
    rpc_client: &RpcClient,
    payer: &Keypair,
    app_pubkey: &Pubkey,
    mint: &Pubkey,
    amount: u64,
    recipient: Option<&Pubkey>,
) -> Result<DepositResult> {
    let owner = payer.pubkey();
    let dest = recipient.unwrap_or(&owner);

    let source_ata = get_associated_token_address(&owner, mint);
    let dest_ata = get_associated_token_address(app_pubkey, mint);

    // Create a buffer account (ephemeral keypair for deposit correlation).
    let buffer = Keypair::new();
    let buffer_pubkey = buffer.pubkey();

    // Build the SPL transfer instruction.
    let transfer_ix = spl_token::instruction::transfer(
        &spl_token::id(),
        &source_ata,
        &dest_ata,
        &owner,
        &[],
        amount,
    )
    .map_err(|e| NordError::Solana(format!("build transfer ix: {e}")))?;

    // Create a minimal account for the buffer (to correlate the deposit).
    let rent = rpc_client
        .get_minimum_balance_for_rent_exemption(0)
        .map_err(|e| NordError::Solana(format!("get rent: {e}")))?;

    let create_buffer_ix = system_instruction::create_account(
        &owner,
        &buffer_pubkey,
        rent,
        0,
        &solana_sdk::system_program::id(),
    );

    // Memo instruction to tag the destination.
    let memo_program_id: Pubkey = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"
        .parse()
        .map_err(|e| NordError::Solana(format!("parse memo program: {e}")))?;
    let memo_ix = solana_sdk::instruction::Instruction {
        program_id: memo_program_id,
        accounts: vec![solana_sdk::instruction::AccountMeta::new_readonly(
            owner, true,
        )],
        data: dest.to_bytes().to_vec(),
    };

    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .map_err(|e| NordError::Solana(format!("get blockhash: {e}")))?;

    let tx = Transaction::new_signed_with_payer(
        &[create_buffer_ix, transfer_ix, memo_ix],
        Some(&owner),
        &[payer, &buffer],
        recent_blockhash,
    );

    let signature = rpc_client
        .send_and_confirm_transaction(&tx)
        .map_err(|e| NordError::Solana(format!("send tx: {e}")))?;

    Ok(DepositResult {
        signature: signature.to_string(),
        buffer: buffer_pubkey,
    })
}
