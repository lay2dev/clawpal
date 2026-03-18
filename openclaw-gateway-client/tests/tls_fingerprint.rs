use openclaw_gateway_client::client::GatewayClientBuilder;
use openclaw_gateway_client::tls::normalize_fingerprint;

#[test]
fn normalizes_sha256_fingerprint_variants() {
    assert_eq!(
        normalize_fingerprint("AA:bb:cc"),
        Some("AA:BB:CC".into())
    );
    assert_eq!(
        normalize_fingerprint("aabbcc"),
        Some("AA:BB:CC".into())
    );
    assert_eq!(normalize_fingerprint(""), None);
}

#[test]
fn rejects_non_hex_fingerprint() {
    assert_eq!(normalize_fingerprint("zz:11"), None);
}

#[test]
fn fingerprint_requires_wss_url() {
    let err = GatewayClientBuilder::new("ws://127.0.0.1:18789")
        .client_id("openclaw-rust")
        .client_mode("node")
        .client_version("0.1.0")
        .platform("linux")
        .role("node")
        .tls_fingerprint("AA:BB")
        .build()
        .expect_err("non-wss should reject fingerprint");

    assert!(
        err.to_string().contains("tls fingerprint requires wss"),
        "unexpected error: {err}"
    );
}
