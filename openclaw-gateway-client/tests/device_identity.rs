use openclaw_gateway_client::identity::{
    build_device_auth_payload_v3, generate_device_identity, sign_device_payload,
};
use serde_json::json;

#[test]
fn generates_device_identity_with_expected_shape() {
    let identity = generate_device_identity().expect("generate identity");

    assert!(!identity.device_id.trim().is_empty());
    assert!(identity.public_key_pem.contains("BEGIN PUBLIC KEY"));
    assert!(identity.private_key_pem.contains("BEGIN PRIVATE KEY"));
}

#[test]
fn builds_device_auth_payload_with_nonce() {
    let payload = build_device_auth_payload_v3(
        "device-1",
        "openclaw-rust",
        "node",
        "node",
        &[],
        123,
        Some("token-1"),
        "nonce-123",
        "linux",
        Some("Linux"),
    );

    assert_eq!(
        payload,
        json!({
            "v": 3,
            "deviceId": "device-1",
            "clientId": "openclaw-rust",
            "clientMode": "node",
            "role": "node",
            "scopes": [],
            "signedAtMs": 123,
            "token": "token-1",
            "nonce": "nonce-123",
            "platform": "linux",
            "deviceFamily": "Linux"
        })
    );
}

#[test]
fn signs_payload_and_returns_base64url_signature() {
    let identity = generate_device_identity().expect("generate identity");
    let payload = build_device_auth_payload_v3(
        &identity.device_id,
        "openclaw-rust",
        "node",
        "node",
        &[],
        123,
        None,
        "nonce-123",
        "linux",
        None,
    );

    let signature = sign_device_payload(&identity.private_key_pem, &payload).expect("sign payload");

    assert!(!signature.trim().is_empty());
    assert!(!signature.contains('='));
    assert!(!signature.contains('+'));
    assert!(!signature.contains('/'));
}
