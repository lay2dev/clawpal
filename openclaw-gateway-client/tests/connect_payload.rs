use openclaw_gateway_client::protocol::{
    AuthPayload, ClientInfo, ConnectParams, DeviceAuth, EventFrame, GatewayFrame, HelloOk,
    PolicyInfo, ResponseFrame,
};
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn deserializes_connect_challenge_event() {
    let raw = json!({
        "type": "event",
        "event": "connect.challenge",
        "payload": { "nonce": "nonce-123" }
    });

    let frame: GatewayFrame = serde_json::from_value(raw).expect("deserialize challenge");

    let GatewayFrame::Event(EventFrame { event, payload, .. }) = frame else {
        panic!("expected event frame");
    };

    assert_eq!(event, "connect.challenge");
    assert_eq!(payload, Some(json!({ "nonce": "nonce-123" })));
}

#[test]
fn serializes_connect_params() {
    let params = ConnectParams {
        min_protocol: 3,
        max_protocol: 3,
        client: ClientInfo {
            id: "openclaw-rust".into(),
            display_name: Some("Rust Node".into()),
            version: "0.1.0".into(),
            platform: "linux".into(),
            mode: "node".into(),
            instance_id: Some("node-1".into()),
            device_family: Some("Linux".into()),
            model_identifier: None,
        },
        caps: vec!["system".into()],
        commands: Some(vec!["system.run".into(), "system.which".into()]),
        permissions: None,
        path_env: Some("/usr/bin".into()),
        auth: Some(AuthPayload {
            token: Some("shared-token".into()),
            device_token: Some("device-token".into()),
            password: None,
        }),
        role: "node".into(),
        scopes: vec![],
        device: Some(DeviceAuth {
            id: "device-1".into(),
            public_key: "pub".into(),
            signature: "sig".into(),
            signed_at: 123,
            nonce: "nonce-123".into(),
        }),
        locale: Some("en-US".into()),
        user_agent: Some("OpenClawRust/0.1.0".into()),
    };

    let encoded = serde_json::to_value(&params).expect("serialize connect params");

    assert_eq!(
        encoded,
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "openclaw-rust",
                "displayName": "Rust Node",
                "version": "0.1.0",
                "platform": "linux",
                "mode": "node",
                "instanceId": "node-1",
                "deviceFamily": "Linux"
            },
            "caps": ["system"],
            "commands": ["system.run", "system.which"],
            "pathEnv": "/usr/bin",
            "auth": {
                "token": "shared-token",
                "deviceToken": "device-token"
            },
            "role": "node",
            "scopes": [],
            "device": {
                "id": "device-1",
                "publicKey": "pub",
                "signature": "sig",
                "signedAt": 123,
                "nonce": "nonce-123"
            },
            "locale": "en-US",
            "userAgent": "OpenClawRust/0.1.0"
        })
    );
}

#[test]
fn deserializes_hello_ok_response_payload() {
    let raw = json!({
        "type": "res",
        "id": "req-1",
        "ok": true,
        "payload": {
            "serverName": "gateway.local",
            "policy": { "tickIntervalMs": 30000 },
            "auth": {
                "deviceToken": "next-device-token",
                "role": "node",
                "scopes": []
            },
            "snapshot": {
                "health": {},
                "presence": []
            }
        }
    });

    let frame: GatewayFrame = serde_json::from_value(raw).expect("deserialize hello response");

    let GatewayFrame::Response(ResponseFrame {
        payload: Some(payload),
        ..
    }) = frame
    else {
        panic!("expected response");
    };

    let hello: HelloOk = serde_json::from_value(payload).expect("decode hello payload");
    assert_eq!(hello.server_name.as_deref(), Some("gateway.local"));
    assert_eq!(hello.policy.tick_interval_ms, Some(30_000));
    assert_eq!(
        hello.auth.and_then(|auth| auth.device_token),
        Some("next-device-token".into())
    );
}

#[test]
fn serializes_policy_info_in_hello_shape() {
    let hello = HelloOk {
        server_name: Some("gateway.local".into()),
        policy: PolicyInfo {
            tick_interval_ms: Some(15_000),
        },
        auth: None,
        snapshot: None,
        canvas_host_url: None,
    };

    let encoded = serde_json::to_value(&hello).expect("serialize hello");

    assert_eq!(
        encoded,
        json!({
            "serverName": "gateway.local",
            "policy": {
                "tickIntervalMs": 15000
            }
        })
    );
}
