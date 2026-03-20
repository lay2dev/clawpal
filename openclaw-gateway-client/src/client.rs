use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;
use uuid::Uuid;

use crate::error::Error;
use crate::protocol::{
    ClientInfo, ConnectParams, EventFrame, GatewayFrame, HelloOk, RequestFrame, ResponseFrame,
    PROTOCOL_VERSION,
};
use crate::tls::normalize_fingerprint;

#[derive(Debug, Clone)]
pub struct GatewayClient {
    url: Url,
    connect_params: ConnectParams,
    _tls_fingerprint: Option<String>,
}

#[derive(Debug)]
pub struct GatewayClientHandle {
    inner: Arc<GatewayClientInner>,
}

#[derive(Debug)]
struct GatewayClientInner {
    writer: Mutex<
        futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
    >,
    pending: Mutex<std::collections::HashMap<String, oneshot::Sender<Result<Value, Error>>>>,
    events: broadcast::Sender<EventFrame>,
    task: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug, Default)]
pub struct GatewayClientBuilder {
    url: Option<String>,
    client_id: Option<String>,
    client_mode: Option<String>,
    client_version: Option<String>,
    platform: Option<String>,
    role: Option<String>,
    tls_fingerprint: Option<String>,
}

impl GatewayClientBuilder {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: Some(url.into()),
            ..Self::default()
        }
    }

    pub fn client_id(mut self, value: impl Into<String>) -> Self {
        self.client_id = Some(value.into());
        self
    }

    pub fn client_mode(mut self, value: impl Into<String>) -> Self {
        self.client_mode = Some(value.into());
        self
    }

    pub fn client_version(mut self, value: impl Into<String>) -> Self {
        self.client_version = Some(value.into());
        self
    }

    pub fn platform(mut self, value: impl Into<String>) -> Self {
        self.platform = Some(value.into());
        self
    }

    pub fn role(mut self, value: impl Into<String>) -> Self {
        self.role = Some(value.into());
        self
    }

    pub fn tls_fingerprint(mut self, value: impl Into<String>) -> Self {
        self.tls_fingerprint = Some(value.into());
        self
    }

    pub fn build(self) -> Result<GatewayClient, Error> {
        let url = Url::parse(
            &self
                .url
                .ok_or_else(|| Error::Config("url is required".into()))?,
        )
        .map_err(|err| Error::Config(err.to_string()))?;
        let tls_fingerprint = self
            .tls_fingerprint
            .as_deref()
            .and_then(normalize_fingerprint);
        if tls_fingerprint.is_some() && url.scheme() != "wss" {
            return Err(Error::Config(
                "tls fingerprint requires wss gateway url".into(),
            ));
        }
        let role = self.role.unwrap_or_else(|| "operator".into());
        let mode = self.client_mode.unwrap_or_else(|| "backend".into());
        let connect_params = ConnectParams {
            min_protocol: PROTOCOL_VERSION,
            max_protocol: PROTOCOL_VERSION,
            client: ClientInfo {
                id: self.client_id.unwrap_or_else(|| "openclaw-rust".into()),
                display_name: None,
                version: self.client_version.unwrap_or_else(|| "dev".into()),
                platform: self.platform.unwrap_or_else(|| std::env::consts::OS.into()),
                mode,
                instance_id: None,
                device_family: None,
                model_identifier: None,
            },
            caps: Vec::new(),
            commands: None,
            permissions: None,
            path_env: None,
            auth: None,
            role,
            scopes: Vec::new(),
            device: None,
            locale: None,
            user_agent: None,
        };
        Ok(GatewayClient {
            url,
            connect_params,
            _tls_fingerprint: tls_fingerprint,
        })
    }
}

impl GatewayClient {
    pub async fn start(self) -> Result<GatewayClientHandle, Error> {
        let (ws, _) = connect_async(self.url.as_str())
            .await
            .map_err(|err| Error::Transport(err.to_string()))?;
        let (writer, mut reader) = ws.split();

        let challenge = read_until_challenge(&mut reader).await?;
        let nonce = challenge
            .payload
            .and_then(|payload| {
                payload
                    .get("nonce")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| Error::Protocol("connect challenge missing nonce".into()))?;

        let request_id = Uuid::new_v4().to_string();
        let connect_frame = GatewayFrame::Request(RequestFrame {
            id: request_id.clone(),
            method: "connect".into(),
            params: Some(serde_json::to_value(self.build_connect_params(&nonce))?),
        });

        {
            let mut locked = writer;
            locked
                .send(Message::text(serde_json::to_string(&connect_frame)?))
                .await
                .map_err(|err| Error::Transport(err.to_string()))?;
            let _hello = read_until_connect_response(&mut reader, &request_id).await?;
            let writer = locked;
            let (events_tx, _) = broadcast::channel(256);
            let inner = Arc::new(GatewayClientInner {
                writer: Mutex::new(writer),
                pending: Mutex::new(std::collections::HashMap::new()),
                events: events_tx,
                task: Mutex::new(None),
            });

            let read_inner = Arc::clone(&inner);
            let task = tokio::spawn(async move {
                while let Some(next) = reader.next().await {
                    match next {
                        Ok(message) => {
                            if !message.is_text() {
                                continue;
                            }
                            let Ok(frame) = serde_json::from_str::<GatewayFrame>(
                                message.to_text().unwrap_or_default(),
                            ) else {
                                continue;
                            };
                            match frame {
                                GatewayFrame::Response(response) => {
                                    let sender =
                                        read_inner.pending.lock().await.remove(&response.id);
                                    if let Some(sender) = sender {
                                        let result = if response.ok {
                                            Ok(response.payload.unwrap_or(Value::Null))
                                        } else {
                                            Err(Error::Protocol(
                                                response
                                                    .error
                                                    .and_then(|value| {
                                                        value
                                                            .get("message")
                                                            .and_then(|msg| msg.as_str())
                                                            .map(str::to_string)
                                                    })
                                                    .unwrap_or_else(|| "request failed".into()),
                                            ))
                                        };
                                        let _ = sender.send(result);
                                    }
                                }
                                GatewayFrame::Event(event) => {
                                    let _ = read_inner.events.send(event);
                                }
                                GatewayFrame::Request(_) => {}
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            *inner.task.lock().await = Some(task);

            return Ok(GatewayClientHandle { inner });
        }
    }

    fn build_connect_params(&self, _nonce: &str) -> ConnectParams {
        self.connect_params.clone()
    }
}

impl GatewayClientHandle {
    pub async fn request(&self, method: &str, params: Option<Value>) -> Result<Value, Error> {
        let id = Uuid::new_v4().to_string();
        let frame = GatewayFrame::Request(RequestFrame {
            id: id.clone(),
            method: method.to_string(),
            params,
        });
        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().await.insert(id, tx);
        {
            let mut writer = self.inner.writer.lock().await;
            writer
                .send(Message::text(serde_json::to_string(&frame)?))
                .await
                .map_err(|err| Error::Transport(err.to_string()))?;
        }
        rx.await
            .map_err(|_| Error::Protocol("request canceled".into()))?
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<EventFrame> {
        self.inner.events.subscribe()
    }

    pub async fn shutdown(self) -> Result<(), Error> {
        {
            let mut writer = self.inner.writer.lock().await;
            let _ = writer.send(Message::Close(None)).await;
        }
        if let Some(task) = self.inner.task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        Ok(())
    }
}

impl Clone for GatewayClientHandle {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

async fn read_until_challenge(
    reader: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> Result<EventFrame, Error> {
    while let Some(message) = reader.next().await {
        let message = message.map_err(|err| Error::Transport(err.to_string()))?;
        if !message.is_text() {
            continue;
        }
        let frame: GatewayFrame = serde_json::from_str(
            message
                .to_text()
                .map_err(|err| Error::Transport(err.to_string()))?,
        )?;
        if let GatewayFrame::Event(event) = frame {
            if event.event == "connect.challenge" {
                return Ok(event);
            }
        }
    }
    Err(Error::Protocol(
        "connection closed before connect.challenge".into(),
    ))
}

async fn read_until_connect_response(
    reader: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    request_id: &str,
) -> Result<HelloOk, Error> {
    while let Some(message) = reader.next().await {
        let message = message.map_err(|err| Error::Transport(err.to_string()))?;
        if !message.is_text() {
            continue;
        }
        let frame: GatewayFrame = serde_json::from_str(
            message
                .to_text()
                .map_err(|err| Error::Transport(err.to_string()))?,
        )?;
        if let GatewayFrame::Response(ResponseFrame {
            id,
            ok,
            payload,
            error,
        }) = frame
        {
            if id != request_id {
                continue;
            }
            if !ok {
                return Err(Error::Protocol(
                    error
                        .and_then(|value| {
                            value
                                .get("message")
                                .and_then(|msg| msg.as_str())
                                .map(str::to_string)
                        })
                        .unwrap_or_else(|| "connect failed".into()),
                ));
            }
            let payload = payload
                .ok_or_else(|| Error::Protocol("connect response missing payload".into()))?;
            return serde_json::from_value(payload).map_err(Error::from);
        }
    }
    Err(Error::Protocol(
        "connection closed before connect response".into(),
    ))
}
