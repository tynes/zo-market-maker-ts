pub mod atomic;
pub mod session;
pub mod signing;

use prost::Message;
use std::future::Future;
use std::pin::Pin;

use crate::error::{NordError, Result};
use crate::proto::nord::{self, Action, Receipt};
use crate::rest::NordHttpClient;

/// Signing function trait object type.
pub type SignFn =
    dyn Fn(&[u8]) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send>> + Send + Sync;

/// Create an action with the given timestamp, nonce, and kind.
pub fn create_action(timestamp: u64, nonce: u32, kind: nord::action::Kind) -> Action {
    Action {
        current_timestamp: timestamp as i64,
        nonce,
        kind: Some(kind),
    }
}

/// Encode action as length-delimited protobuf, sign it, and concatenate.
pub async fn prepare_action(action: &Action, sign_fn: &SignFn) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    action
        .encode_length_delimited(&mut raw)
        .map_err(NordError::ProtobufEncode)?;

    let signature = sign_fn(&raw).await?;

    let mut msg = raw;
    msg.extend_from_slice(&signature);
    Ok(msg)
}

/// Send a prepared action to the server and decode the receipt.
pub async fn send_action(
    http_client: &NordHttpClient,
    action: &Action,
    sign_fn: &SignFn,
) -> Result<Receipt> {
    let payload = prepare_action(action, sign_fn).await?;
    let response_bytes = http_client.post_action(&payload).await?;
    let receipt = Receipt::decode_length_delimited(response_bytes.as_slice())
        .map_err(NordError::ProtobufDecode)?;
    Ok(receipt)
}

/// Format a receipt error into a human-readable string.
pub fn format_receipt_error(receipt: &Receipt) -> String {
    match &receipt.kind {
        Some(nord::receipt::Kind::Err(code)) => {
            format!("Receipt error code {code}")
        }
        _ => "Unknown receipt error".to_string(),
    }
}

/// Assert that a receipt contains the expected kind, or return an error.
pub fn expect_receipt_kind(receipt: &Receipt, expected: &str) -> Result<()> {
    match &receipt.kind {
        Some(nord::receipt::Kind::Err(code)) => Err(NordError::ReceiptError(format!(
            "Expected {expected}, got error code {code}"
        ))),
        Some(_) => Ok(()),
        None => Err(NordError::ReceiptError(format!(
            "Expected {expected}, got empty receipt"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_action_fields() {
        let timestamp = 1700000000u64;
        let nonce = 42u32;
        let kind = nord::action::Kind::CancelOrderById(nord::action::CancelOrderById {
            session_id: 0,
            order_id: 999,
            delegator_account_id: None,
            sender_account_id: None,
        });

        let action = create_action(timestamp, nonce, kind);

        assert_eq!(action.current_timestamp, 1700000000i64);
        assert_eq!(action.nonce, 42);
        assert!(action.kind.is_some());
    }

    #[test]
    fn test_create_action_timestamp_cast() {
        // Ensure u64 -> i64 cast works for typical timestamps
        let ts = 1_700_000_000u64;
        let action = create_action(
            ts,
            0,
            nord::action::Kind::CancelOrderById(nord::action::CancelOrderById {
                session_id: 0,
                order_id: 1,
                delegator_account_id: None,
                sender_account_id: None,
            }),
        );
        assert_eq!(action.current_timestamp, ts as i64);
    }

    #[test]
    fn test_create_action_cancel_order_kind() {
        let action = create_action(
            0,
            1,
            nord::action::Kind::CancelOrderById(nord::action::CancelOrderById {
                session_id: 0,
                order_id: 12345,
                delegator_account_id: None,
                sender_account_id: None,
            }),
        );

        match action.kind.unwrap() {
            nord::action::Kind::CancelOrderById(c) => {
                assert_eq!(c.order_id, 12345);
            }
            _ => panic!("expected CancelOrderById kind"),
        }
    }

    #[test]
    fn test_create_action_encodes_to_protobuf() {
        let action = create_action(
            1_000_000,
            7,
            nord::action::Kind::CancelOrderById(nord::action::CancelOrderById {
                session_id: 0,
                order_id: 1,
                delegator_account_id: None,
                sender_account_id: None,
            }),
        );

        let mut buf = Vec::new();
        action.encode_length_delimited(&mut buf).unwrap();

        // Should produce non-empty protobuf bytes
        assert!(!buf.is_empty());

        // Should round-trip decode
        let decoded = Action::decode_length_delimited(buf.as_slice()).unwrap();
        assert_eq!(decoded.current_timestamp, 1_000_000);
        assert_eq!(decoded.nonce, 7);
    }

    #[test]
    fn test_format_receipt_error() {
        let receipt = Receipt {
            action_id: 0,
            kind: Some(nord::receipt::Kind::Err(42)),
        };
        let msg = format_receipt_error(&receipt);
        assert!(msg.contains("42"));
    }

    #[test]
    fn test_format_receipt_error_unknown() {
        let receipt = Receipt {
            action_id: 0,
            kind: None,
        };
        let msg = format_receipt_error(&receipt);
        assert_eq!(msg, "Unknown receipt error");
    }

    #[test]
    fn test_expect_receipt_kind_err() {
        let receipt = Receipt {
            action_id: 0,
            kind: Some(nord::receipt::Kind::Err(1)),
        };
        let result = expect_receipt_kind(&receipt, "PlaceOrder");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("PlaceOrder"));
    }

    #[test]
    fn test_expect_receipt_kind_none() {
        let receipt = Receipt {
            action_id: 0,
            kind: None,
        };
        let result = expect_receipt_kind(&receipt, "Anything");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("empty receipt"));
    }
}
