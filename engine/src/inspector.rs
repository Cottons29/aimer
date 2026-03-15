//! Widget Inspector — remote debugging service.
//!
//! When enabled, this module starts a WebSocket server (default port 9229).
//! The CLI connects to it and can toggle inspection on/off via F12.
//! When active, the engine serialises the widget tree after each frame and
//! broadcasts the JSON snapshot to every connected client.

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
pub mod server {
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
    use tokio::net::TcpListener;
    use tokio::sync::broadcast;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::Utf8Bytes;
    use futures_util::sink::SinkExt;
    use futures_util::stream::StreamExt;
    use serde::{Deserialize, Serialize};
    use widget::Element;

    pub const DEFAULT_PORT: u16 = 9229;

    /// A node in the serialised widget tree.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    pub struct WidgetNode {
        pub name: String,
        pub x: f32,
        pub y: f32,
        pub width: f32,
        pub height: f32,
        pub children: Vec<WidgetNode>,
    }

    /// Top-level inspector message sent over the WebSocket.
    #[derive(Serialize, Deserialize, Clone, Debug)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum InspectorMessage {
        /// Full widget tree snapshot.
        Tree { root: Option<WidgetNode> },
        /// Inspector enabled/disabled status.
        Status { enabled: bool },
    }

    /// Shared inspector state accessible from the render loop.
    #[derive(Clone)]
    pub struct InspectorHandle {
        pub enabled: Arc<AtomicBool>,
        tx: broadcast::Sender<String>,
    }

    impl InspectorHandle {
        /// Returns `true` if the inspector is currently active.
        pub fn is_enabled(&self) -> bool {
            self.enabled.load(Ordering::Relaxed)
        }

        /// Toggle the inspector on/off and broadcast the new status.
        pub fn set_enabled(&self, enabled: bool) {
            self.enabled.store(enabled, Ordering::Relaxed);
            widget::inspector_overlay::set_enabled(enabled);
            let msg = InspectorMessage::Status { enabled };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = self.tx.send(json);
            }
        }

        /// Broadcast a widget tree snapshot to all connected CLI clients.
        pub fn broadcast_tree(&self, root: Option<WidgetNode>) {
            if !self.is_enabled() {
                return;
            }
            let msg = InspectorMessage::Tree { root };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = self.tx.send(json);
            }
        }
    }

    /// Start the WebSocket inspector server on the given port.
    /// Returns an `InspectorHandle` that the render loop uses to push snapshots.
    pub fn start(port: u16, runtime: &tokio::runtime::Handle) -> InspectorHandle {
        let (tx, _rx) = broadcast::channel::<String>(64);
        let enabled = Arc::new(AtomicBool::new(false));

        let handle = InspectorHandle { enabled: enabled.clone(), tx: tx.clone() };

        let tx_server = tx.clone();
        let enabled_server = enabled.clone();
        runtime.spawn(async move {
            let addr = format!("127.0.0.1:{}", port);
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[inspector] failed to bind {}: {}", addr, e);
                    return;
                }
            };
            println!("[inspector] WebSocket server listening on ws://{}", addr);

            loop {
                let (stream, peer) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let tx_conn = tx_server.clone();
                let enabled_conn = enabled_server.clone();
                tokio::spawn(async move {
                    let ws = match accept_async(stream).await {
                        Ok(ws) => ws,
                        Err(e) => {
                            eprintln!("[inspector] WS handshake error from {}: {}", peer, e);
                            return;
                        }
                    };

                    println!("[inspector] client connected: {}", peer);

                    let (mut write, mut read) = ws.split();
                    let mut rx = tx_conn.subscribe();

                    // Send current status immediately on connect
                    let status = InspectorMessage::Status { enabled: enabled_conn.load(Ordering::Relaxed) };
                    if let Ok(json) = serde_json::to_string(&status) {
                        let _ = write.send(Message::Text(Utf8Bytes::from(json.as_str()))).await;
                    }

                    loop {
                        tokio::select! {
                            // Outgoing: broadcast messages to this client
                            msg = rx.recv() => {
                                match msg {
                                    Ok(json) => {
                                        if write.send(Message::Text(Utf8Bytes::from(json.as_str()))).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                            // Incoming: commands from CLI (e.g. toggle)
                            incoming = read.next() => {
                                match incoming {
                                    Some(Ok(Message::Text(text))) => {
                                        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                                            if cmd.get("type").and_then(|v| v.as_str()) == Some("toggle") {
                                                let new_val = !enabled_conn.load(Ordering::Relaxed);
                                                enabled_conn.store(new_val, Ordering::Relaxed);
                                                widget::inspector_overlay::set_enabled(new_val);
                                                let status_msg = InspectorMessage::Status { enabled: new_val };
                                                if let Ok(json) = serde_json::to_string(&status_msg) {
                                                    let _ = tx_conn.send(json);
                                                }
                                            }
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) | None => break,
                                    _ => {}
                                }
                            }
                        }
                    }

                    println!("[inspector] client disconnected: {}", peer);
                });
            }
        });

        handle
    }

    /// Recursively walk the element tree and build a `WidgetNode` snapshot.
    pub fn snapshot_tree(element: &dyn Element) -> WidgetNode {
        let (x, y, width, height) = if let Some((start, end)) = element.pos_start_end() {
            (start.x, start.y, end.x - start.x, end.y - start.y)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let mut children = Vec::new();
        element.event_children(&mut |child| {
            children.push(snapshot_tree(child));
        });

        WidgetNode {
            name: element.debug_name().to_string(),
            x,
            y,
            width,
            height,
            children,
        }
    }
}
