use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

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

// /// Handle to the inspector background thread and shared state.
// pub struct InspectorClient {
//     pub state: Arc<Mutex<InspectorState>>,
//     cmd_tx: std::sync::mpsc::Sender<String>,
// }
