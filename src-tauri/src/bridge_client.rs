use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Mutex};

use crate::models::resolve_paths;

/// Commands that the bridge node advertises to the gateway.
const BRIDGE_COMMANDS: &[&str] = &[
    "read_file",
    "list_files",
    "read_config",
    "system_info",
    "validate_config",
    "write_file",
    "run_command",
];

/// Maximum number of pending invoke requests kept in memory.
const MAX_PENDING_INVOKES: usize = 50;

/// Path (relative to clawpal_dir) where the bridge token is persisted.
const TOKEN_FILE: &str = "bridge-token.json";

#[allow(dead_code)]
struct BridgeClientInner {
    writer: WriteHalf<TcpStream>,
    node_id: String,
    token: Option<String>,
}

pub struct BridgeClient {
    inner: Arc<Mutex<Option<BridgeClientInner>>>,
    pending_invokes: Arc<Mutex<HashMap<String, Value>>>,
}

impl BridgeClient {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            pending_invokes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Connect to the openclaw gateway bridge port using raw TCP + NDJSON.
    /// Performs the pair-request or hello handshake, then spawns a reader task.
    pub async fn connect(&self, addr: &str, app: AppHandle) -> Result<(), String> {
        // Disconnect any existing connection
        self.disconnect().await?;

        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| format!("Bridge TCP connection failed: {e}"))?;

        let (reader, writer) = tokio::io::split(stream);

        let node_id = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "clawpal-unknown".into());

        let saved_token = load_token();

        let inner = BridgeClientInner {
            writer,
            node_id: node_id.clone(),
            token: saved_token.clone(),
        };

        {
            let mut guard = self.inner.lock().await;
            *guard = Some(inner);
        }

        // Create oneshot channels for handshake synchronization.
        // The reader task will send through these when it receives pair-ok or hello-ok.
        let (pair_ok_tx, pair_ok_rx) = oneshot::channel::<String>(); // sends token
        let (hello_ok_tx, hello_ok_rx) = oneshot::channel::<Value>();

        // Wrap senders in Arc<Mutex<Option<...>>> so the reader task can take them once.
        let pair_ok_tx = Arc::new(Mutex::new(Some(pair_ok_tx)));
        let hello_ok_tx = Arc::new(Mutex::new(Some(hello_ok_tx)));

        // Spawn reader task
        let inner_ref = Arc::clone(&self.inner);
        let invokes_ref = Arc::clone(&self.pending_invokes);
        let app_clone = app.clone();
        let pair_ok_tx_clone = Arc::clone(&pair_ok_tx);
        let hello_ok_tx_clone = Arc::clone(&hello_ok_tx);

        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let line = line.trim().to_string();
                        if line.is_empty() {
                            continue;
                        }
                        eprintln!("[bridge] <<< {}", &line[..line.len().min(500)]);
                        match serde_json::from_str::<Value>(&line) {
                            Ok(frame) => {
                                Self::handle_frame(
                                    frame,
                                    &inner_ref,
                                    &invokes_ref,
                                    &app_clone,
                                    &pair_ok_tx_clone,
                                    &hello_ok_tx_clone,
                                )
                                .await;
                            }
                            Err(e) => {
                                eprintln!("[bridge] invalid JSON frame: {e}");
                            }
                        }
                    }
                    Ok(None) => {
                        // EOF — connection closed
                        let _ = app_clone.emit(
                            "doctor:bridge-disconnected",
                            json!({"reason": "connection closed"}),
                        );
                        let mut guard = inner_ref.lock().await;
                        *guard = None;
                        break;
                    }
                    Err(e) => {
                        let _ = app_clone.emit(
                            "doctor:error",
                            json!({"message": format!("Bridge read error: {e}")}),
                        );
                        let _ = app_clone.emit(
                            "doctor:bridge-disconnected",
                            json!({"reason": format!("{e}")}),
                        );
                        let mut guard = inner_ref.lock().await;
                        *guard = None;
                        break;
                    }
                }
            }
        });

        // Perform handshake
        self.do_handshake(saved_token, &node_id, pair_ok_rx, hello_ok_rx, &app)
            .await?;

        Ok(())
    }

    pub async fn disconnect(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().await;
        if let Some(mut inner) = guard.take() {
            let _ = inner.writer.shutdown().await;
        }
        self.pending_invokes.lock().await.clear();
        Ok(())
    }

    pub async fn is_connected(&self) -> bool {
        self.inner.lock().await.is_some()
    }

    /// Send a successful invoke result back to the gateway.
    pub async fn send_invoke_result(&self, req_id: &str, result: Value) -> Result<(), String> {
        let frame = json!({
            "type": "invoke-res",
            "id": req_id,
            "ok": true,
            "payload": result,
        });
        self.send_frame(&frame).await
    }

    /// Send an error invoke result back to the gateway.
    pub async fn send_invoke_error(
        &self,
        req_id: &str,
        code: &str,
        message: &str,
    ) -> Result<(), String> {
        let frame = json!({
            "type": "invoke-res",
            "id": req_id,
            "ok": false,
            "error": {
                "code": code,
                "message": message,
            },
        });
        self.send_frame(&frame).await
    }

    /// Take a pending invoke request by ID (removes it from the map).
    pub async fn take_invoke(&self, id: &str) -> Option<Value> {
        self.pending_invokes.lock().await.remove(id)
    }

    // ── Private helpers ──────────────────────────────────────────────

    /// Send a JSON frame as NDJSON (JSON + newline) over the TCP connection.
    async fn send_frame(&self, frame: &Value) -> Result<(), String> {
        let mut guard = self.inner.lock().await;
        let inner = guard.as_mut().ok_or("Bridge not connected")?;
        let mut data = serde_json::to_string(frame).map_err(|e| format!("JSON serialize error: {e}"))?;
        data.push('\n');
        eprintln!("[bridge] >>> {}", &data[..data.len().min(500)]);
        inner
            .writer
            .write_all(data.as_bytes())
            .await
            .map_err(|e| format!("Bridge write error: {e}"))?;
        inner
            .writer
            .flush()
            .await
            .map_err(|e| format!("Bridge flush error: {e}"))?;
        Ok(())
    }

    /// Perform the pair-request or hello handshake sequence.
    async fn do_handshake(
        &self,
        saved_token: Option<String>,
        node_id: &str,
        pair_ok_rx: oneshot::Receiver<String>,
        hello_ok_rx: oneshot::Receiver<Value>,
        app: &AppHandle,
    ) -> Result<(), String> {
        let version = env!("CARGO_PKG_VERSION");
        let commands: Vec<&str> = BRIDGE_COMMANDS.to_vec();

        if let Some(token) = saved_token {
            // Have token — send hello directly
            let hello_frame = json!({
                "type": "hello",
                "nodeId": node_id,
                "displayName": "ClawPal",
                "token": token,
                "platform": "macos",
                "version": version,
                "deviceFamily": "desktop",
                "commands": commands,
            });
            self.send_frame(&hello_frame).await?;

            // Wait for hello-ok (30s timeout)
            match tokio::time::timeout(std::time::Duration::from_secs(30), hello_ok_rx).await {
                Ok(Ok(_)) => {
                    let _ = app.emit("doctor:bridge-connected", json!({}));
                    Ok(())
                }
                Ok(Err(_)) => {
                    // Channel dropped — connection lost during handshake
                    Err("Connection lost during hello handshake".into())
                }
                Err(_) => {
                    Err("Hello handshake timed out (30s)".into())
                }
            }
        } else {
            // No token — send pair-request first
            let _ = app.emit("doctor:bridge-pairing", json!({}));

            let pair_frame = json!({
                "type": "pair-request",
                "nodeId": node_id,
                "displayName": "ClawPal",
                "platform": "macos",
                "version": version,
                "deviceFamily": "desktop",
                "commands": commands,
                "silent": true,
            });
            self.send_frame(&pair_frame).await?;

            // Wait for pair-ok (6 min timeout — user may need to approve on gateway)
            let token = match tokio::time::timeout(
                std::time::Duration::from_secs(360),
                pair_ok_rx,
            )
            .await
            {
                Ok(Ok(token)) => token,
                Ok(Err(_)) => {
                    return Err("Connection lost during pairing".into());
                }
                Err(_) => {
                    return Err("Pairing timed out (6 min)".into());
                }
            };

            // Save the token
            save_token(&token);

            // Update inner state with the new token
            {
                let mut guard = self.inner.lock().await;
                if let Some(inner) = guard.as_mut() {
                    inner.token = Some(token.clone());
                }
            }

            // Now send hello with the new token
            let hello_frame = json!({
                "type": "hello",
                "nodeId": node_id,
                "displayName": "ClawPal",
                "token": token,
                "platform": "macos",
                "version": version,
                "deviceFamily": "desktop",
                "commands": commands,
            });
            self.send_frame(&hello_frame).await?;

            // Wait for hello-ok (30s timeout)
            match tokio::time::timeout(std::time::Duration::from_secs(30), hello_ok_rx).await {
                Ok(Ok(_)) => {
                    let _ = app.emit("doctor:bridge-connected", json!({}));
                    Ok(())
                }
                Ok(Err(_)) => {
                    Err("Connection lost during hello handshake".into())
                }
                Err(_) => {
                    Err("Hello handshake timed out (30s)".into())
                }
            }
        }
    }

    /// Handle a single parsed JSON frame from the bridge connection.
    async fn handle_frame(
        frame: Value,
        inner_ref: &Arc<Mutex<Option<BridgeClientInner>>>,
        invokes_ref: &Arc<Mutex<HashMap<String, Value>>>,
        app: &AppHandle,
        pair_ok_tx: &Arc<Mutex<Option<oneshot::Sender<String>>>>,
        hello_ok_tx: &Arc<Mutex<Option<oneshot::Sender<Value>>>>,
    ) {
        let frame_type = frame.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match frame_type {
            "pair-ok" => {
                let token = frame
                    .get("token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // Signal the handshake task that pairing succeeded
                if let Some(tx) = pair_ok_tx.lock().await.take() {
                    let _ = tx.send(token);
                }
            }
            "hello-ok" => {
                // Signal the handshake task that hello succeeded
                if let Some(tx) = hello_ok_tx.lock().await.take() {
                    let _ = tx.send(frame.clone());
                }
            }
            "ping" => {
                let id = frame.get("id").cloned().unwrap_or(Value::Null);
                let pong = json!({
                    "type": "pong",
                    "id": id,
                });
                // Send pong — need to lock inner and write
                let mut guard = inner_ref.lock().await;
                if let Some(inner) = guard.as_mut() {
                    let mut data = serde_json::to_string(&pong).unwrap_or_default();
                    data.push('\n');
                    eprintln!("[bridge] >>> {}", &data[..data.len().min(500)]);
                    let _ = inner.writer.write_all(data.as_bytes()).await;
                    let _ = inner.writer.flush().await;
                }
            }
            "invoke" => {
                let id = frame
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let command = frame
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let args = frame.get("args").cloned().unwrap_or(Value::Null);

                // Determine type from command name
                let cmd_type = match command.as_str() {
                    "read_file" | "list_files" | "read_config" | "system_info"
                    | "validate_config" => "read",
                    _ => "write",
                };

                let invoke_payload = json!({
                    "id": id,
                    "command": command,
                    "args": args,
                    "type": cmd_type,
                });

                // Store for later approval/rejection (bounded to MAX_PENDING_INVOKES)
                {
                    let mut map = invokes_ref.lock().await;
                    if map.len() >= MAX_PENDING_INVOKES {
                        // Evict arbitrary entries to make room (likely stale)
                        let keys: Vec<String> = map.keys().take(10).cloned().collect();
                        for k in keys {
                            map.remove(&k);
                        }
                    }
                    map.insert(id.clone(), invoke_payload.clone());
                }

                let _ = app.emit("doctor:invoke", invoke_payload);
            }
            "error" => {
                let message = frame
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown bridge error");
                let _ = app.emit("doctor:error", json!({"message": message}));
            }
            _ => {
                eprintln!("[bridge] unhandled frame type: {frame_type}");
            }
        }
    }
}

impl Default for BridgeClient {
    fn default() -> Self {
        Self::new()
    }
}

// ── Token persistence ────────────────────────────────────────────────

fn token_path() -> std::path::PathBuf {
    let paths = resolve_paths();
    paths.clawpal_dir.join(TOKEN_FILE)
}

fn load_token() -> Option<String> {
    let path = token_path();
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: Value = serde_json::from_str(&content).ok()?;
    parsed.get("token").and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn save_token(token: &str) {
    let path = token_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let data = json!({"token": token});
    if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&data).unwrap_or_default()) {
        eprintln!("[bridge] failed to save token to {:?}: {e}", path);
    }
}
