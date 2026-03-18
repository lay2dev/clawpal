use futures::{SinkExt, StreamExt};
use openclaw_gateway_client::client::GatewayClientBuilder;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn waits_for_connect_challenge_and_sends_connect_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let (tx, rx) = oneshot::channel::<Value>();

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

        let message = ws.next().await.expect("message").expect("websocket message");
        let text = message.into_text().expect("text frame");
        let value: Value = serde_json::from_str(&text).expect("json request");
        let req_id = value["id"].as_str().expect("request id").to_string();
        tx.send(value).expect("capture request");

        ws.send(Message::text(
            json!({
                "type": "res",
                "id": req_id,
                "ok": true,
                "payload": {
                    "serverName": "gateway.local",
                    "policy": { "tickIntervalMs": 30000 }
                }
            })
            .to_string(),
        ))
        .await
        .expect("send hello");
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
    let request = rx.await.expect("captured connect request");

    assert_eq!(request["type"], "req");
    assert_eq!(request["method"], "connect");
    assert_eq!(request["params"]["role"], "node");
    assert_eq!(request["params"]["client"]["id"], "openclaw-rust");
    assert_eq!(request["params"]["minProtocol"], 3);
    assert_eq!(request["params"]["maxProtocol"], 3);

    handle.shutdown().await.expect("shutdown");
    server.await.expect("server task");
}
