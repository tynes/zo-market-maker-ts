use prost::Message;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use crate::error::{NordError, Result};
use crate::types::{MarketInfo, TokenInfo};

/// Convert a Decimal to a scaled u64 value.
pub fn to_scaled_u64(x: Decimal, decimals: u32) -> u64 {
    let scale = Decimal::from(10u64.pow(decimals));
    let scaled = x * scale;
    scaled.to_u64().expect("to_scaled_u64: value out of range")
}

/// Convert a Decimal to a scaled u128 value.
pub fn to_scaled_u128(x: Decimal, decimals: u32) -> u128 {
    let scale = Decimal::from(10u64.pow(decimals));
    let scaled = x * scale;
    scaled.to_u128().expect("to_scaled_u128: value out of range")
}

/// Decode a length-delimited protobuf message from bytes.
pub fn decode_length_delimited<T: Message + Default>(bytes: &[u8]) -> Result<T> {
    T::decode_length_delimited(bytes).map_err(NordError::ProtobufDecode)
}

/// Decode a hex string (with optional `0x` prefix) to bytes.
pub fn decode_hex(value: &str) -> Vec<u8> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    hex::decode(stripped).expect("invalid hex string")
}

/// Find a market by its ID.
pub fn find_market(markets: &[MarketInfo], id: u32) -> Result<&MarketInfo> {
    markets
        .iter()
        .find(|m| m.market_id == id)
        .ok_or(NordError::MarketNotFound(id))
}

/// Find a token by its ID.
pub fn find_token(tokens: &[TokenInfo], id: u32) -> Result<&TokenInfo> {
    tokens
        .iter()
        .find(|t| t.token_id == id)
        .ok_or(NordError::TokenNotFound(id))
}

/// Parse a private key from a bs58 string or raw bytes.
pub fn keypair_from_private_key(
    key: &str,
) -> Result<ed25519_dalek::SigningKey> {
    let bytes = bs58::decode(key)
        .into_vec()
        .map_err(|e| NordError::Signing(format!("bs58 decode error: {e}")))?;

    // Accept either a 32-byte secret or a 64-byte keypair (first 32 bytes are the secret).
    let secret_bytes: [u8; 32] = if bytes.len() == 64 {
        bytes[..32]
            .try_into()
            .map_err(|_| NordError::Signing("invalid key length".into()))?
    } else if bytes.len() == 32 {
        bytes
            .try_into()
            .map_err(|_| NordError::Signing("invalid key length".into()))?
    } else {
        return Err(NordError::Signing(format!(
            "unexpected key length: {}",
            bytes.len()
        )));
    };

    Ok(ed25519_dalek::SigningKey::from_bytes(&secret_bytes))
}

/// Check if a string looks like an RFC 3339 timestamp.
pub fn is_rfc3339(s: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(s).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    // ---- to_scaled_u64 ----

    #[test]
    fn test_to_scaled_u64_basic() {
        assert_eq!(to_scaled_u64(dec!(1.5), 2), 150);
        assert_eq!(to_scaled_u64(dec!(0.001), 6), 1000);
        assert_eq!(to_scaled_u64(dec!(100), 0), 100);
    }

    #[test]
    fn test_to_scaled_u64_zero() {
        assert_eq!(to_scaled_u64(dec!(0), 6), 0);
        assert_eq!(to_scaled_u64(dec!(0.0), 2), 0);
    }

    #[test]
    fn test_to_scaled_u64_large_decimals() {
        // 1.23456789 * 10^8 = 123456789
        assert_eq!(to_scaled_u64(dec!(1.23456789), 8), 123456789);
    }

    #[test]
    fn test_to_scaled_u64_whole_number_with_decimals() {
        // 42 * 10^6 = 42_000_000
        assert_eq!(to_scaled_u64(dec!(42), 6), 42_000_000);
    }

    #[test]
    #[should_panic(expected = "value out of range")]
    fn test_to_scaled_u64_negative_panics() {
        to_scaled_u64(dec!(-1.0), 2);
    }

    // ---- to_scaled_u128 ----

    #[test]
    fn test_to_scaled_u128_basic() {
        assert_eq!(to_scaled_u128(dec!(1.5), 2), 150);
        assert_eq!(to_scaled_u128(dec!(999999999.999), 3), 999999999999);
    }

    #[test]
    fn test_to_scaled_u128_zero() {
        assert_eq!(to_scaled_u128(dec!(0), 18), 0);
    }

    #[test]
    fn test_to_scaled_u128_large_value() {
        // Large value that would overflow u64 but fits in u128:
        // 1_000_000_000 * 10^10 = 10^19
        let result = to_scaled_u128(dec!(1000000000), 10);
        assert_eq!(result, 10_000_000_000_000_000_000u128);
    }

    #[test]
    fn test_to_scaled_u128_quote_size_scenario() {
        // Typical use: price * size with combined decimals
        // price=50000.50, size=0.1, combined_decimals = 2+4 = 6
        // value = 5000.050, scaled by 10^6 = 5_000_050_000
        let value = dec!(50000.50) * dec!(0.1);
        assert_eq!(to_scaled_u128(value, 6), 5000050000);
    }

    // ---- decode_hex ----

    #[test]
    fn test_decode_hex_with_0x_prefix() {
        assert_eq!(decode_hex("0x0102"), vec![1, 2]);
        assert_eq!(decode_hex("0xff"), vec![255]);
    }

    #[test]
    fn test_decode_hex_without_prefix() {
        assert_eq!(decode_hex("abcd"), vec![0xab, 0xcd]);
        assert_eq!(decode_hex("00"), vec![0]);
    }

    #[test]
    fn test_decode_hex_empty() {
        assert_eq!(decode_hex(""), Vec::<u8>::new());
        assert_eq!(decode_hex("0x"), Vec::<u8>::new());
    }

    #[test]
    fn test_decode_hex_uppercase() {
        assert_eq!(decode_hex("0xABCD"), vec![0xab, 0xcd]);
        assert_eq!(decode_hex("ABCD"), vec![0xab, 0xcd]);
    }

    #[test]
    #[should_panic(expected = "invalid hex string")]
    fn test_decode_hex_invalid_panics() {
        decode_hex("0xGG");
    }

    #[test]
    #[should_panic(expected = "invalid hex string")]
    fn test_decode_hex_odd_length_panics() {
        decode_hex("abc");
    }

    // ---- is_rfc3339 ----

    #[test]
    fn test_is_rfc3339_valid() {
        assert!(is_rfc3339("2024-01-15T10:30:00Z"));
        assert!(is_rfc3339("2024-12-31T23:59:59.999Z"));
        assert!(is_rfc3339("2024-01-15T10:30:00+05:30"));
    }

    #[test]
    fn test_is_rfc3339_invalid() {
        assert!(!is_rfc3339("not a date"));
        assert!(!is_rfc3339("2024-01-15"));
        assert!(!is_rfc3339(""));
        assert!(!is_rfc3339("1234567890"));
    }

    // ---- find_market / find_token ----

    fn sample_markets() -> Vec<MarketInfo> {
        vec![
            MarketInfo {
                market_id: 1,
                symbol: "BTCUSDC".into(),
                price_decimals: 2,
                size_decimals: 4,
                base_token_id: 0,
                quote_token_id: 1,
                imf: 0.1,
                mmf: 0.05,
                cmf: 0.03,
            },
            MarketInfo {
                market_id: 2,
                symbol: "ETHUSDC".into(),
                price_decimals: 2,
                size_decimals: 3,
                base_token_id: 2,
                quote_token_id: 1,
                imf: 0.1,
                mmf: 0.05,
                cmf: 0.03,
            },
        ]
    }

    fn sample_tokens() -> Vec<TokenInfo> {
        vec![
            TokenInfo {
                token_id: 0,
                symbol: "BTC".into(),
                decimals: 8,
                mint_addr: "btc_mint".into(),
                weight_bps: 5000,
            },
            TokenInfo {
                token_id: 1,
                symbol: "USDC".into(),
                decimals: 6,
                mint_addr: "usdc_mint".into(),
                weight_bps: 10000,
            },
        ]
    }

    #[test]
    fn test_find_market_found() {
        let markets = sample_markets();
        let m = find_market(&markets, 1).unwrap();
        assert_eq!(m.symbol, "BTCUSDC");
    }

    #[test]
    fn test_find_market_second() {
        let markets = sample_markets();
        let m = find_market(&markets, 2).unwrap();
        assert_eq!(m.symbol, "ETHUSDC");
    }

    #[test]
    fn test_find_market_not_found_error_message() {
        let markets = sample_markets();
        let err = find_market(&markets, 99).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("99"), "error should mention market id 99: {msg}");
    }

    #[test]
    fn test_find_market_empty_list() {
        let err = find_market(&[], 1).unwrap_err();
        assert!(matches!(err, NordError::MarketNotFound(1)));
    }

    #[test]
    fn test_find_token_found() {
        let tokens = sample_tokens();
        let t = find_token(&tokens, 1).unwrap();
        assert_eq!(t.symbol, "USDC");
    }

    #[test]
    fn test_find_token_not_found_error_message() {
        let tokens = sample_tokens();
        let err = find_token(&tokens, 42).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("42"), "error should mention token id 42: {msg}");
    }

    #[test]
    fn test_find_token_empty_list() {
        let err = find_token(&[], 0).unwrap_err();
        assert!(matches!(err, NordError::TokenNotFound(0)));
    }

    // ---- keypair_from_private_key ----

    #[test]
    fn test_keypair_from_private_key_32_bytes() {
        // A known 32-byte secret key, bs58-encoded
        let secret = [1u8; 32];
        let encoded = bs58::encode(&secret).into_string();
        let key = keypair_from_private_key(&encoded).unwrap();
        assert_eq!(key.to_bytes(), secret);
    }

    #[test]
    fn test_keypair_from_private_key_64_bytes() {
        // ed25519 keypair: 32 bytes secret + 32 bytes public
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[2u8; 32]);
        let mut keypair_bytes = [0u8; 64];
        keypair_bytes[..32].copy_from_slice(&signing_key.to_bytes());
        keypair_bytes[32..].copy_from_slice(signing_key.verifying_key().as_bytes());
        let encoded = bs58::encode(&keypair_bytes).into_string();
        let key = keypair_from_private_key(&encoded).unwrap();
        assert_eq!(key.to_bytes(), [2u8; 32]);
    }

    #[test]
    fn test_keypair_from_private_key_bad_length() {
        let bad = bs58::encode(&[0u8; 16]).into_string();
        assert!(keypair_from_private_key(&bad).is_err());
    }

    #[test]
    fn test_keypair_from_private_key_invalid_bs58() {
        assert!(keypair_from_private_key("!!!invalid!!!").is_err());
    }
}
