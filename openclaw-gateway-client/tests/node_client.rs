use futures::{SinkExt, StreamExt};
use openclaw_gateway_client::client::GatewayClientBuilder;
use openclaw_gateway_client::node::NodeClient;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn node_client_decodes_invoke_requests_and_sends_results_and_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let (capture_tx, capture_rx) = oneshot::channel::<(Value, Value)>();

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

        ws.send(Message::text(
            json!({
                "type": "event",
                "event": "node.invoke.request",
                "payload": {
                    "id": "invoke-1",
                    "nodeId": "node-1",
                    "command": "debug.echo",
                    "params": { "hello": "world" },
                    "timeoutMs": 5000
                }
            })
            .to_string(),
        ))
        .await
        .expect("send invoke request");

        let invoke_result_text = ws
            .next()
            .await
            .expect("invoke result message")
            .expect("invoke result frame")
            .into_text()
            .expect("text");
        let invoke_result_value: Value =
            serde_json::from_str(&invoke_result_text).expect("invoke result json");
        let invoke_result_id = invoke_result_value["id"]
            .as_str()
            .expect("invoke result id")
            .to_string();

        ws.send(Message::text(
            json!({
                "type": "res",
                "id": invoke_result_id,
                "ok": true,
                "payload": { "status": "ok" }
            })
            .to_string(),
        ))
        .await
        .expect("ack invoke result");

        let node_event_text = ws
            .next()
            .await
            .expect("node event message")
            .expect("node event frame")
            .into_text()
            .expect("text");
        let node_event_value: Value = serde_json::from_str(&node_event_text).expect("node event json");
        let node_event_id = node_event_value["id"]
            .as_str()
            .expect("node event id")
            .to_string();

        ws.send(Message::text(
            json!({
                "type": "res",
                "id": node_event_id,
                "ok": true,
                "payload": { "status": "ok" }
            })
            .to_string(),
        ))
        .await
        .expect("ack node event");

        capture_tx
            .send((invoke_result_value, node_event_value))
            .expect("capture values");
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
    let node = NodeClient::new(handle.clone());

    let invoke = node.next_invoke().await.expect("invoke request");
    assert_eq!(invoke.id, "invoke-1");
    assert_eq!(invoke.node_id, "node-1");
    assert_eq!(invoke.command, "debug.echo");
    assert_eq!(invoke.params, Some(json!({ "hello": "world" })));

    node.send_invoke_result(&invoke, true, Some(json!({ "echoed": true })), None)
        .await
        .expect("send invoke result");

    node.send_event("exec.finished", Some(json!({ "runId": "run-1" })))
        .await
        .expect("send node event");

    let (invoke_result, node_event) = capture_rx.await.expect("capture channel");
    assert_eq!(invoke_result["method"], "node.invoke.result");
    assert_eq!(invoke_result["params"]["id"], "invoke-1");
    assert_eq!(invoke_result["params"]["nodeId"], "node-1");
    assert_eq!(invoke_result["params"]["ok"], true);
    assert_eq!(invoke_result["params"]["payload"], json!({ "echoed": true }));

    assert_eq!(node_event["method"], "node.event");
    assert_eq!(node_event["params"]["event"], "exec.finished");
    assert_eq!(node_event["params"]["payload"], json!({ "runId": "run-1" }));

    handle.shutdown().await.expect("shutdown");
    server.await.expect("server task");
}
