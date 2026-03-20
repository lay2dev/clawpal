use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::GatewayClientHandle;
use crate::error::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInvokeRequest {
    pub id: String,
    pub node_id: String,
    pub command: String,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct NodeClient {
    handle: GatewayClientHandle,
}

impl NodeClient {
    pub fn new(handle: GatewayClientHandle) -> Self {
        Self { handle }
    }

    pub async fn next_invoke(&self) -> Result<NodeInvokeRequest, Error> {
        let mut events = self.handle.subscribe_events();
        loop {
            let event = events
                .recv()
                .await
                .map_err(|_| Error::Protocol("event stream closed".into()))?;
            if event.event != "node.invoke.request" {
                continue;
            }
            let payload = event
                .payload
                .ok_or_else(|| Error::Protocol("node.invoke.request missing payload".into()))?;
            return serde_json::from_value(payload).map_err(Error::from);
        }
    }

    pub async fn send_invoke_result(
        &self,
        request: &NodeInvokeRequest,
        ok: bool,
        payload: Option<Value>,
        error: Option<Value>,
    ) -> Result<Value, Error> {
        let mut params = serde_json::Map::new();
        params.insert("id".into(), Value::String(request.id.clone()));
        params.insert("nodeId".into(), Value::String(request.node_id.clone()));
        params.insert("ok".into(), Value::Bool(ok));
        if let Some(payload) = payload {
            params.insert("payload".into(), payload);
        }
        if let Some(error) = error {
            params.insert("error".into(), error);
        }
        self.handle
            .request("node.invoke.result", Some(Value::Object(params)))
            .await
    }

    pub async fn send_event(&self, event: &str, payload: Option<Value>) -> Result<Value, Error> {
        let mut params = serde_json::Map::new();
        params.insert("event".into(), Value::String(event.to_string()));
        if let Some(payload) = payload {
            params.insert("payload".into(), payload);
        }
        self.handle
            .request("node.event", Some(Value::Object(params)))
            .await
    }
}
