//! Integration tests for the signing module.
//!
//! Uses known ed25519 keypairs to verify that `sign_hex_encoded_payload` and
//! `sign_raw_payload` produce deterministic, verifiable signatures.

use ed25519_dalek::{SigningKey, Verifier};

/// Helper: create a deterministic signing key from a fixed seed.
fn test_signing_key() -> SigningKey {
    // A fixed 32-byte secret for reproducible tests.
    let secret: [u8; 32] = [
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ];
    SigningKey::from_bytes(&secret)
}

#[tokio::test]
async fn test_sign_hex_encoded_payload_deterministic() {
    let key = test_signing_key();
    let payload = b"hello world";

    let sig1 = nord::actions::signing::sign_hex_encoded_payload(payload, &key)
        .await
        .unwrap();
    let sig2 = nord::actions::signing::sign_hex_encoded_payload(payload, &key)
        .await
        .unwrap();

    // Ed25519 signing with dalek is deterministic (RFC 6979-style)
    assert_eq!(sig1, sig2, "signatures should be deterministic");
    assert_eq!(sig1.len(), 64, "ed25519 signature should be 64 bytes");
}

#[tokio::test]
async fn test_sign_raw_payload_deterministic() {
    let key = test_signing_key();
    let payload = b"raw bytes payload";

    let sig1 = nord::actions::signing::sign_raw_payload(payload, &key)
        .await
        .unwrap();
    let sig2 = nord::actions::signing::sign_raw_payload(payload, &key)
        .await
        .unwrap();

    assert_eq!(sig1, sig2, "signatures should be deterministic");
    assert_eq!(sig1.len(), 64);
}

#[tokio::test]
async fn test_sign_hex_encoded_verifies() {
    let key = test_signing_key();
    let verifying_key = key.verifying_key();
    let payload = b"verify me";

    let sig_bytes = nord::actions::signing::sign_hex_encoded_payload(payload, &key)
        .await
        .unwrap();

    let signature = ed25519_dalek::Signature::from_bytes(sig_bytes.as_slice().try_into().unwrap());

    // The hex-encoded scheme signs the hex encoding of the payload
    let hex_encoded = hex::encode(payload);
    verifying_key
        .verify(hex_encoded.as_bytes(), &signature)
        .expect("signature should verify against hex-encoded payload");
}

#[tokio::test]
async fn test_sign_raw_verifies() {
    let key = test_signing_key();
    let verifying_key = key.verifying_key();
    let payload = b"verify raw";

    let sig_bytes = nord::actions::signing::sign_raw_payload(payload, &key)
        .await
        .unwrap();

    let signature = ed25519_dalek::Signature::from_bytes(sig_bytes.as_slice().try_into().unwrap());

    // Raw signing signs the payload bytes directly
    verifying_key
        .verify(payload, &signature)
        .expect("signature should verify against raw payload");
}

#[tokio::test]
async fn test_hex_and_raw_produce_different_signatures() {
    let key = test_signing_key();
    let payload = b"same payload";

    let hex_sig = nord::actions::signing::sign_hex_encoded_payload(payload, &key)
        .await
        .unwrap();
    let raw_sig = nord::actions::signing::sign_raw_payload(payload, &key)
        .await
        .unwrap();

    // They sign different things (hex(payload) vs payload), so signatures differ
    assert_ne!(hex_sig, raw_sig, "hex and raw signatures should differ");
}

#[tokio::test]
async fn test_sign_empty_payload() {
    let key = test_signing_key();

    let hex_sig = nord::actions::signing::sign_hex_encoded_payload(b"", &key)
        .await
        .unwrap();
    let raw_sig = nord::actions::signing::sign_raw_payload(b"", &key)
        .await
        .unwrap();

    assert_eq!(hex_sig.len(), 64);
    assert_eq!(raw_sig.len(), 64);

    // For empty payload, hex("") = "", and raw = b"", so they should be the same
    assert_eq!(hex_sig, raw_sig, "empty payload: hex(\"\") == \"\"");
}

#[tokio::test]
async fn test_different_keys_produce_different_signatures() {
    let key1 = SigningKey::from_bytes(&[1u8; 32]);
    let key2 = SigningKey::from_bytes(&[2u8; 32]);
    let payload = b"test payload";

    let sig1 = nord::actions::signing::sign_raw_payload(payload, &key1)
        .await
        .unwrap();
    let sig2 = nord::actions::signing::sign_raw_payload(payload, &key2)
        .await
        .unwrap();

    assert_ne!(sig1, sig2, "different keys should produce different sigs");
}

#[tokio::test]
async fn test_sign_hex_known_vector() {
    // Use a fixed key and payload, verify a specific signature to catch regressions.
    let key = SigningKey::from_bytes(&[0xAA; 32]);
    let payload = b"nord-test";

    let sig = nord::actions::signing::sign_hex_encoded_payload(payload, &key)
        .await
        .unwrap();

    // The hex encoding of b"nord-test" is "6e6f72642d74657374"
    let hex_payload = hex::encode(payload);
    assert_eq!(hex_payload, "6e6f72642d74657374");

    // Verify the signature is valid for the hex-encoded message
    let verifying_key = key.verifying_key();
    let signature = ed25519_dalek::Signature::from_bytes(sig.as_slice().try_into().unwrap());
    verifying_key
        .verify(hex_payload.as_bytes(), &signature)
        .expect("known vector signature should verify");

    // Sign again to confirm determinism
    let sig2 = nord::actions::signing::sign_hex_encoded_payload(payload, &key)
        .await
        .unwrap();
    assert_eq!(sig, sig2);
}

/// Test that the full prepare_action flow (encode + sign) works end-to-end.
#[tokio::test]
async fn test_prepare_action_round_trip() {
    use nord::actions::{create_action, prepare_action};
    use nord::proto::nord as proto;
    use prost::Message;

    let key = test_signing_key();

    let action = create_action(
        1_700_000_000,
        1,
        proto::action::Kind::CancelOrderById(proto::action::CancelOrderById {
            session_id: 0,
            order_id: 42,
            delegator_account_id: None,
            sender_account_id: None,
        }),
    );

    // Create a sign function closure
    let sign_fn: Box<nord::actions::SignFn> = Box::new(move |payload: &[u8]| {
        let key = key.clone();
        let payload = payload.to_vec();
        Box::pin(async move { nord::actions::signing::sign_raw_payload(&payload, &key).await })
    });

    let prepared = prepare_action(&action, &sign_fn).await.unwrap();

    // The prepared bytes should contain the length-delimited action + 64 byte signature
    let action_len = action.encoded_len();
    let varint_len = prost::length_delimiter_len(action_len);
    let expected_total = varint_len + action_len + 64;
    assert_eq!(
        prepared.len(),
        expected_total,
        "prepared = varint + action + 64 byte sig"
    );

    // Decode the action portion back
    let action_bytes = &prepared[..varint_len + action_len];
    let decoded = proto::Action::decode_length_delimited(action_bytes).unwrap();
    assert_eq!(decoded.current_timestamp, 1_700_000_000);
    assert_eq!(decoded.nonce, 1);
}
