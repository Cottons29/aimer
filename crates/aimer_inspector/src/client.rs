use std::sync::{Arc, Mutex};
use std::thread;

use tungstenite::Message;

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
                                    InspectorMessage::Hovered { id } => {
                                        s.hovered_widget_id = id;
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

/// Render the widget tree as indented text lines for display in ratatui,
/// using a tree-style layout with box-drawing characters (├──, └──, │).
/// When `full_tree` is true, each node shows `ElementType: WidgetName`;
/// otherwise only the widget name is displayed.
pub fn render_tree_lines(
    node: &WidgetNode,
    _depth: usize,
    lines: &mut Vec<String>,
    full_tree: bool,
) {
    let mut ids = Vec::new();
    render_tree_recursive(node, lines, &mut ids, "", full_tree);
}

pub fn render_tree_lines_with_ids(
    node: &WidgetNode,
    lines: &mut Vec<String>,
    ids: &mut Vec<u64>,
    full_tree: bool,
) {
    render_tree_recursive(node, lines, ids, "", full_tree);
}

fn node_label(node: &WidgetNode, full_tree: bool) -> String {
    let pos_info = if node.width > 0.0 || node.height > 0.0 {
        format!("  [{:.0}×{:.0} @ ({:.0},{:.0})]", node.width, node.height, node.x, node.y)
    } else {
        String::new()
    };
    if full_tree && !node.element_type.is_empty() {
        format!("{}  ({}){}", node.name, node.element_type, pos_info)
    } else {
        format!("{}{}", node.name, pos_info)
    }
}

fn render_tree_recursive(
    node: &WidgetNode,
    lines: &mut Vec<String>,
    ids: &mut Vec<u64>,
    prefix: &str,
    full_tree: bool,
) {
    lines.push(format!("{}{}", prefix, node_label(node, full_tree)));
    ids.push(node.id);

    let count = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let continuation = if is_last { "    " } else { "│   " };

        let child_prefix = format!("{}{}", prefix, connector);
        let grandchild_base = format!("{}{}", prefix, continuation);

        render_tree_with_base(child, lines, ids, &child_prefix, &grandchild_base, full_tree);
    }
}

fn render_tree_with_base(
    node: &WidgetNode,
    lines: &mut Vec<String>,
    ids: &mut Vec<u64>,
    line_prefix: &str,
    child_base: &str,
    full_tree: bool,
) {
    lines.push(format!("{}{}", line_prefix, node_label(node, full_tree)));
    ids.push(node.id);

    let count = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let continuation = if is_last { "    " } else { "│   " };

        let child_line_prefix = format!("{}{}", child_base, connector);
        let grandchild_base = format!("{}{}", child_base, continuation);

        render_tree_with_base(child, lines, ids, &child_line_prefix, &grandchild_base, full_tree);
    }
}
