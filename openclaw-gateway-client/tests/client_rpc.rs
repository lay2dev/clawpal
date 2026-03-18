use futures::{SinkExt, StreamExt};
use openclaw_gateway_client::client::GatewayClientBuilder;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn request_receives_matching_response_and_events_are_broadcast() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let (req_tx, mut req_rx) = mpsc::unbounded_channel::<Value>();
    let (ready_tx, ready_rx) = oneshot::channel::<()>();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut ws = accept_async(stream).await.expect("accept websocket");
        ws.send(Message::text(
            json!({
                "type": "event",
                "event": "connect.challenge",
                "payload": { "nonce": "nonce-123" }
            })
            .to_string(),
        ))
        .await
        .expect("send challenge");

        let connect_text = ws
            .next()
            .await
            .expect("connect message")
            .expect("connect frame")
            .into_text()
            .expect("text");
        let connect_value: Value = serde_json::from_str(&connect_text).expect("connect json");
        let connect_id = connect_value["id"].as_str().expect("connect id").to_string();

        ws.send(Message::text(
            json!({
                "type": "res",
                "id": connect_id,
                "ok": true,
                "payload": {
                    "serverName": "gateway.local",
                    "policy": { "tickIntervalMs": 30000 }
                }
            })
            .to_string(),
        ))
        .await
        .expect("send connect response");

        ready_tx.send(()).expect("ready");

        let request_text = ws
            .next()
            .await
            .expect("rpc message")
            .expect("rpc frame")
            .into_text()
            .expect("text");
        let request_value: Value = serde_json::from_str(&request_text).expect("rpc json");
        req_tx.send(request_value.clone()).expect("request capture");
        let req_id = request_value["id"].as_str().expect("rpc id").to_string();

        ws.send(Message::text(
            json!({
                "type": "event",
                "event": "test.event",
                "payload": { "ok": true },
                "seq": 1
            })
            .to_string(),
        ))
        .await
        .expect("send event");

        ws.send(Message::text(
            json!({
                "type": "res",
                "id": req_id,
                "ok": true,
                "payload": { "result": 42 }
            })
            .to_string(),
        ))
        .await
        .expect("send rpc response");
    });

    let client = GatewayClientBuilder::new(format!("ws://{}", addr))
        .client_id("openclaw-rust")
        .client_mode("node")
        .client_version("0.1.0")
        .platform("linux")
        .role("node")
        .build()
        .expect("build client");

    let handle = client.start().await.expect("start client");
    ready_rx.await.expect("handshake ready");

    let mut events = handle.subscribe_events();
    let response = handle
        .request("debug.echo", Some(json!({ "hello": "world" })))
        .await
        .expect("rpc response");

    assert_eq!(response, json!({ "result": 42 }));

    let sent_request = req_rx.recv().await.expect("captured request");
    assert_eq!(sent_request["type"], "req");
    assert_eq!(sent_request["method"], "debug.echo");
    assert_eq!(sent_request["params"], json!({ "hello": "world" }));

    let event = events.recv().await.expect("event");
    assert_eq!(event.event, "test.event");
    assert_eq!(event.payload, Some(json!({ "ok": true })));

    handle.shutdown().await.expect("shutdown");
    server.await.expect("server task");
}
