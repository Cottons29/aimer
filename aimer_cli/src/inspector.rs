//! Widget Inspector panel for the aimer CLI.
//!
//! Connects to the engine's WebSocket inspector server (ws://127.0.0.1:9229)
//! and displays the live widget tree. Toggle with F12 in the console.

use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use tungstenite::{Message, handshake::client::Request};
use serde::{Deserialize, Serialize};

pub const INSPECTOR_PORT: u16 = 9229;

/// Mirror of the engine's WidgetNode for deserialisation.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WidgetNode {
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub children: Vec<WidgetNode>,
}

/// Mirror of the engine's InspectorMessage.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InspectorMessage {
    Tree { root: Option<WidgetNode> },
    Status { enabled: bool },
}

/// Shared state updated by the background WebSocket thread.
#[derive(Clone, Default)]
pub struct InspectorState {
    pub connected: bool,
    pub enabled: bool,
    pub tree: Option<WidgetNode>,
}

/// Handle to the inspector background thread and shared state.
pub struct InspectorClient {
    pub state: Arc<Mutex<InspectorState>>,
    cmd_tx: std::sync::mpsc::Sender<String>,
}

impl InspectorClient {
    /// Spawn the background WebSocket client thread and return the handle.
    pub fn connect(port: u16) -> Self {
        let state = Arc::new(Mutex::new(InspectorState::default()));
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<String>();

        let state_bg = Arc::clone(&state);
        thread::spawn(move || {
            loop {
                let addr = format!("127.0.0.1:{}", port);
                let tcp = match TcpStream::connect(&addr) {
                    Ok(t) => t,
                    Err(_) => {
                        thread::sleep(std::time::Duration::from_secs(2));
                        continue;
                    }
                };
                let url = format!("ws://{}", addr);
                let request = match Request::builder()
                    .uri(&url)
                    .header("Host", &addr)
                    .header("Connection", "Upgrade")
                    .header("Upgrade", "websocket")
                    .header("Sec-WebSocket-Version", "13")
                    .header("Sec-WebSocket-Key", tungstenite::handshake::client::generate_key())
                    .body(())
                {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                match tungstenite::client(request, tcp) {
                    Ok((mut ws, _)) => {
                        {
                            let mut s = state_bg.lock().unwrap();
                            s.connected = true;
                        }
                        // Set non-blocking so we can interleave sends and receives
                        let _ = ws.get_mut().set_nonblocking(true);
                        loop {
                            // Send any pending commands first
                            while let Ok(cmd) = cmd_rx.try_recv() {
                                let _ = ws.send(Message::Text(cmd.into()));
                            }

                            match ws.read() {
                                Ok(Message::Text(text)) => {
                                    if let Ok(msg) = serde_json::from_str::<InspectorMessage>(&text) {
                                        let mut s = state_bg.lock().unwrap();
                                        match msg {
                                            InspectorMessage::Tree { root } => {
                                                s.tree = root;
                                            }
                                            InspectorMessage::Status { enabled } => {
                                                s.enabled = enabled;
                                            }
                                        }
                                    }
                                }
                                Ok(Message::Close(_)) => break,
                                Err(tungstenite::Error::Io(ref e))
                                    if e.kind() == std::io::ErrorKind::WouldBlock =>
                                {
                                    // No data yet — yield briefly and retry
                                    thread::sleep(std::time::Duration::from_millis(16));
                                }
                                Err(_) => break,
                                _ => {}
                            }
                        }
                        {
                            let mut s = state_bg.lock().unwrap();
                            s.connected = false;
                        }
                    }
                    Err(_) => {}
                }
                // Retry after a short delay
                thread::sleep(std::time::Duration::from_secs(2));
            }
        });

        InspectorClient { state, cmd_tx }
    }

    /// Send a toggle command to the engine.
    pub fn send_toggle(&self) {
        let cmd = r#"{"type":"toggle"}"#.to_string();
        let _ = self.cmd_tx.send(cmd);
    }
}

/// Render the widget tree as indented text lines for display in ratatui.
pub fn render_tree_lines(node: &WidgetNode, depth: usize, lines: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    let pos_info = if node.width > 0.0 || node.height > 0.0 {
        format!("  [{:.0}×{:.0} @ ({:.0},{:.0})]", node.width, node.height, node.x, node.y)
    } else {
        String::new()
    };
    lines.push(format!("{}▸ {}{}", indent, node.name, pos_info));
    for child in &node.children {
        render_tree_lines(child, depth + 1, lines);
    }
}
