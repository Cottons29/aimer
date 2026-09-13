use std::cell::RefCell;
use std::net::IpAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam::channel::{Receiver, Sender, TrySendError, bounded};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};

/// Mirror of the engine's widget tree node used by the CLI inspector.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WidgetNode {
    #[serde(default)]
    pub id: u64,
    pub name: String,
    /// The concrete element type name (e.g. `StatefulElement<Counter>`).
    #[serde(default)]
    pub element_type: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub children: Vec<WidgetNode>,
}

/// Messages exchanged by the CLI inspector transport.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InspectorMessage {
    Tree { root: Option<WidgetNode> },
    Status { enabled: bool },
    Hovered { id: Option<u64> },
}

/// Shared state displayed by the CLI inspector pane.
#[derive(Clone, Default)]
pub struct InspectorState {
    pub connected: bool,
    pub enabled: bool,
    pub tree: Option<WidgetNode>,
    pub hovered_widget_id: Option<u64>,
}

/// A native inspector snapshot cache fed by the WebSocket server task.
#[derive(Clone)]
pub struct InspectorStateStore {
    state: Rc<RefCell<InspectorState>>,
    updates: Receiver<InspectorState>,
}

#[derive(Clone)]
struct InspectorStatePublisher {
    sender: Sender<InspectorState>,
    discard: Receiver<InspectorState>,
}

impl InspectorStateStore {
    fn channel() -> (Self, InspectorStatePublisher) {
        let (sender, updates) = bounded(1);
        (
            Self {
                state: Rc::new(RefCell::new(InspectorState::default())),
                updates: updates.clone(),
            },
            InspectorStatePublisher {
                sender,
                discard: updates,
            },
        )
    }

    /// Returns the newest inspector snapshot without blocking.
    pub fn snapshot(&self) -> InspectorState {
        let mut state = self.state.borrow_mut();
        while let Ok(next) = self.updates.try_recv() {
            *state = next;
        }
        state.clone()
    }

    fn update(&self, update: impl FnOnce(&mut InspectorState)) {
        let mut state = self.state.borrow_mut();
        while let Ok(next) = self.updates.try_recv() {
            *state = next;
        }
        update(&mut state);
    }
}

impl InspectorStatePublisher {
    fn publish(&self, state: InspectorState) {
        match self.sender.try_send(state) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
            Err(TrySendError::Full(state)) => {
                let _ = self.discard.try_recv();
                let _ = self.sender.try_send(state);
            }
        }
    }
}

/// Shared inspector state and transport handle owned by the CLI server.
#[derive(Clone)]
pub struct InspectorHandle {
    pub enabled: Arc<AtomicBool>,
    tx: broadcast::Sender<String>,
    pub port: u16,
    pub address: IpAddr,
    /// Shared state for CLI consumers to read the latest tree and status.
    pub state: InspectorStateStore,
}

impl InspectorHandle {
    /// Returns `true` if the inspector is currently active.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn get_address(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }

    /// Toggle the inspector on/off and broadcast the new status.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        self.state.update(|state| state.enabled = enabled);
        let msg = InspectorMessage::Status { enabled };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.tx.send(json);
        }
    }

    /// Send a toggle command through the broadcast channel.
    pub fn send_toggle(&self) {
        let new_val = !self.enabled.load(Ordering::Relaxed);
        self.enabled.store(new_val, Ordering::Relaxed);
        self.state.update(|state| state.enabled = new_val);
        let msg = InspectorMessage::Status { enabled: new_val };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.tx.send(json);
        }
    }

    /// Broadcast a widget tree snapshot to all connected clients.
    pub fn broadcast_tree(&self, root: Option<WidgetNode>) {
        if !self.is_enabled() {
            return;
        }
        self.state.update(|state| state.tree = root.clone());
        let msg = InspectorMessage::Tree { root };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.tx.send(json);
        }
    }

    /// Broadcast the currently hovered widget ID.
    pub fn broadcast_hovered(&self, id: Option<u64>) {
        self.state.update(|state| state.hovered_widget_id = id);
        let msg = InspectorMessage::Hovered { id };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = self.tx.send(json);
        }
    }
}

/// WebSocket server used by the CLI inspector console.
pub struct InspectorServer;

impl InspectorServer {
    async fn bind_port(address: &str) -> Result<TcpListener, std::io::Error> {
        match TcpListener::bind(address).await {
            Ok(listener) => Ok(listener),
            Err(error) => Err(error),
        }
    }

    /// Start the WebSocket inspector server on the given port.
    /// Returns an [`InspectorHandle`] that the CLI uses to read state and send
    /// commands.
    pub fn start(
        inspector_address: IpAddr,
        inspector_port: u16,
        runtime: &tokio::runtime::Handle,
    ) -> Result<InspectorHandle, std::io::Error> {
        let (tx, _rx) = broadcast::channel::<String>(64);
        let enabled = Arc::new(AtomicBool::new(false));

        let (state, state_publisher) = InspectorStateStore::channel();

        let tx_server = tx.clone();
        let enabled_server = enabled.clone();
        let mut inspector_port_draft = inspector_port;
        let mut retry_count = 0;

        let (listener, handle): (TcpListener, InspectorHandle) = runtime.block_on(async move {
            loop {
                let addr = format!("{inspector_address}:{inspector_port_draft}");
                if let Ok(listener) = Self::bind_port(&addr).await {
                    let handle = InspectorHandle {
                        enabled: enabled.clone(),
                        tx: tx.clone(),
                        state: state.clone(),
                        address: inspector_address,
                        port: inspector_port_draft,
                    };
                    break Ok((listener, handle));
                }

                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                inspector_port_draft += 1;
                retry_count += 1;
                if retry_count > 20 {
                    break Err(std::io::Error::other(
                        "Failed to bind to port after 20 retries",
                    ));
                }
            }
        })?;

        runtime.spawn(async move {
            let mut current_state = InspectorState::default();
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(result) => result,
                    Err(_) => continue,
                };

                let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                    Ok(websocket) => websocket,
                    Err(_) => continue,
                };

                current_state.connected = true;
                state_publisher.publish(current_state.clone());

                let (mut write, mut read) = ws_stream.split();
                let mut rx = tx_server.subscribe();

                let status = InspectorMessage::Status {
                    enabled: enabled_server.load(Ordering::Relaxed),
                };
                if let Ok(json) = serde_json::to_string(&status) {
                    let _ = write
                        .send(Message::Text(Utf8Bytes::from(json.as_str())))
                        .await;
                }

                loop {
                    tokio::select! {
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
                        incoming = read.next() => {
                            match incoming {
                                Some(Ok(Message::Text(text))) => {
                                    if let Ok(msg) = serde_json::from_str::<InspectorMessage>(&text) {
                                        match msg {
                                            InspectorMessage::Tree { root } => {
                                                current_state.tree = root;
                                                state_publisher.publish(current_state.clone());
                                            }
                                            InspectorMessage::Status { enabled } => {
                                                enabled_server.store(enabled, Ordering::Relaxed);
                                                current_state.enabled = enabled;
                                                state_publisher.publish(current_state.clone());
                                            }
                                            InspectorMessage::Hovered { id } => {
                                                current_state.hovered_widget_id = id;
                                                state_publisher.publish(current_state.clone());
                                            }
                                        }
                                    } else {
                                        let Ok(command) = serde_json::from_str::<serde_json::Value>(&text) else {
                                            continue;
                                        };
                                        if command.get("type").and_then(|value| value.as_str()) == Some("toggle") {
                                            let new_val = !enabled_server.load(Ordering::Relaxed);
                                            enabled_server.store(new_val, Ordering::Relaxed);
                                            current_state.enabled = new_val;
                                            state_publisher.publish(current_state.clone());
                                            let status_msg = InspectorMessage::Status { enabled: new_val };
                                            if let Ok(json) = serde_json::to_string(&status_msg) {
                                                let _ = tx_server.send(json);
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

                current_state.connected = false;
                state_publisher.publish(current_state.clone());
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });

        Ok(handle)
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

/// Render a widget tree and collect the node IDs in the same display order.
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
        format!(
            "  [{:.0}×{:.0} @ ({:.0},{:.0})]",
            node.width, node.height, node.x, node.y
        )
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

        render_tree_with_base(
            child,
            lines,
            ids,
            &child_prefix,
            &grandchild_base,
            full_tree,
        );
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

        render_tree_with_base(
            child,
            lines,
            ids,
            &child_line_prefix,
            &grandchild_base,
            full_tree,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{InspectorState, InspectorStateStore};

    #[test]
    fn state_store_keeps_the_newest_snapshot_without_blocking() {
        let (store, publisher) = InspectorStateStore::channel();

        publisher.publish(InspectorState {
            connected: true,
            ..InspectorState::default()
        });
        let latest = InspectorState {
            enabled: true,
            ..InspectorState::default()
        };
        publisher.publish(latest.clone());

        assert_eq!(store.snapshot().enabled, latest.enabled);
        assert_eq!(store.snapshot().connected, latest.connected);
    }
}
