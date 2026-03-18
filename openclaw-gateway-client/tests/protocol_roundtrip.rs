use openclaw_gateway_client::protocol::{EventFrame, GatewayFrame, RequestFrame, ResponseFrame};
use pretty_assertions::assert_eq;
use serde_json::{json, Value};

#[test]
fn serializes_request_frame() {
    let frame = GatewayFrame::Request(RequestFrame {
        id: "req-1".into(),
        method: "chat.send".into(),
        params: Some(json!({ "text": "hello" })),
    });

    let encoded = serde_json::to_value(&frame).expect("serialize request frame");

    assert_eq!(
        encoded,
        json!({
            "type": "req",
            "id": "req-1",
            "method": "chat.send",
            "params": { "text": "hello" }
        })
    );
}

#[test]
fn deserializes_response_frame() {
    let raw = json!({
        "type": "res",
        "id": "req-1",
        "ok": true,
        "payload": { "status": "ok" }
    });

    let frame: GatewayFrame = serde_json::from_value(raw).expect("deserialize response frame");

    let GatewayFrame::Response(ResponseFrame { id, ok, payload, error }) = frame else {
        panic!("expected response frame");
    };

    assert_eq!(id, "req-1");
    assert!(ok);
    assert_eq!(payload, Some(json!({ "status": "ok" })));
    assert_eq!(error, None);
}

#[test]
fn deserializes_event_frame() {
    let raw = json!({
        "type": "event",
        "event": "tick",
        "payload": { "now": 123 },
        "seq": 7
    });

    let frame: GatewayFrame = serde_json::from_value(raw).expect("deserialize event frame");

    let GatewayFrame::Event(EventFrame { event, payload, seq, state_version }) = frame else {
        panic!("expected event frame");
    };

    assert_eq!(event, "tick");
    assert_eq!(payload, Some(json!({ "now": 123 })));
    assert_eq!(seq, Some(7));
    assert_eq!(state_version, None);
}

#[test]
fn omits_absent_optional_fields() {
    let frame = GatewayFrame::Event(EventFrame {
        event: "tick".into(),
        payload: None,
        seq: None,
        state_version: None,
    });

    let encoded = serde_json::to_value(&frame).expect("serialize event frame");

    assert_eq!(encoded, Value::from(json!({ "type": "event", "event": "tick" })));
}
