use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use crossbeam::channel::{Receiver, Sender, TrySendError, bounded};
#[cfg(not(target_arch = "wasm32"))]
use std::cell::RefCell;
#[cfg(not(target_arch = "wasm32"))]
use std::rc::Rc;

/// Mirror of the engine's WidgetNode for deserialisation.
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

/// Mirror of the engine's InspectorMessage.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InspectorMessage {
    Tree { root: Option<WidgetNode> },
    Status { enabled: bool },
    Hovered { id: Option<u64> },
}

/// Shared state updated by the background WebSocket thread.
#[derive(Clone, Default)]
pub struct InspectorState {
    pub connected: bool,
    pub enabled: bool,
    pub tree: Option<WidgetNode>,
    pub hovered_widget_id: Option<u64>,
}

/// A native inspector snapshot cache fed by the WebSocket owner thread.
///
/// The cache has no shared mutable state between the WebSocket task and the
/// CLI. The task publishes immutable snapshots through a one-slot mailbox and
/// the CLI drains the newest snapshot when it renders a frame.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct InspectorStateStore {
    state: Rc<RefCell<InspectorState>>,
    updates: Receiver<InspectorState>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(crate) struct InspectorStatePublisher {
    sender: Sender<InspectorState>,
    discard: Receiver<InspectorState>,
}

#[cfg(not(target_arch = "wasm32"))]
impl InspectorStateStore {
    pub(crate) fn channel() -> (Self, InspectorStatePublisher) {
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

    pub(crate) fn update(&self, update: impl FnOnce(&mut InspectorState)) {
        let mut state = self.state.borrow_mut();
        while let Ok(next) = self.updates.try_recv() {
            *state = next;
        }
        update(&mut state);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl InspectorStatePublisher {
    pub(crate) fn publish(&self, state: InspectorState) {
        match self.sender.try_send(state) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
            Err(TrySendError::Full(state)) => {
                let _ = self.discard.try_recv();
                let _ = self.sender.try_send(state);
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
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
