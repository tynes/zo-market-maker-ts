use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// User profile containing account IDs and active sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub account_ids: Vec<u32>,
    pub sessions: HashMap<String, UserSession>,
}

/// An active authentication session for a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub pubkey: String,
    pub expiry: String,
}

/// Solana SPL token metadata used for deposit/withdrawal operations.
#[derive(Debug, Clone)]
pub struct SPLTokenInfo {
    pub mint: String,
    pub precision: u8,
    pub token_id: u32,
    pub name: String,
}
