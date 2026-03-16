
use std::error::Error;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use tungstenite::Message;
use serde::{Deserialize, Serialize};
use crate::{InspectorMessage, InspectorState, WidgetNode};

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
            let addr = format!("127.0.0.1:{}", port);
            
            loop {
                let tcp = match std::net::TcpStream::connect(&addr) {
                    Ok(s) => s,
                    Err(_) => {
                        thread::sleep(std::time::Duration::from_secs(2));
                        continue;
                    }
                };

                let url = format!("ws://{}", addr);
                let (mut ws, _) = match tungstenite::client(&url, tcp) {
                    Ok(res) => res,
                    Err(_) => {
                        thread::sleep(std::time::Duration::from_secs(2));
                        continue;
                    }
                };

                {
                    let mut s = state_bg.lock().unwrap();
                    s.connected = true;
                }

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
