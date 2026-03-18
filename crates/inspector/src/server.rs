//! Widget Inspector — remote debugging service.
//!
//! The CLI always hosts the WebSocket server (default port 9229).
//! The app (native or WASM) connects to it as a client.
//! When active, the engine serialises the widget tree after each frame and
//! sends the JSON snapshot to the CLI server.

#[cfg(not(target_arch = "wasm32"))]
pub mod server  {
    use crate::{InspectorMessage, InspectorState};
    use futures_util::{SinkExt, StreamExt};
    use std::sync::{
        atomic::{AtomicBool, Ordering}, Arc,
        Mutex,
    };
    use tokio::sync::broadcast;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::Utf8Bytes;
    use widget::Element;

    /// Shared inspector state accessible from the CLI server.
    #[derive(Clone)]
    pub struct InspectorHandle {
        pub enabled: Arc<AtomicBool>,
        tx: broadcast::Sender<String>,
        /// Shared state for CLI consumers to read the latest tree / status.
        pub state: Arc<Mutex<InspectorState>>,
    }

    impl InspectorHandle {
        /// Returns `true` if the inspector is currently active.
        pub fn is_enabled(&self) -> bool {
            self.enabled.load(Ordering::Relaxed)
        }

        /// Toggle the inspector on/off and broadcast the new status.
        pub fn set_enabled(&self, enabled: bool) {
            self.enabled.store(enabled, Ordering::Relaxed);
            {
                let mut s = self.state.lock().unwrap();
                s.enabled = enabled;
            }
            let msg = InspectorMessage::Status { enabled };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = self.tx.send(json);
            }
        }

        /// Send a toggle command through the broadcast channel.
        pub fn send_toggle(&self) {
            let new_val = !self.enabled.load(Ordering::Relaxed);
            self.enabled.store(new_val, Ordering::Relaxed);
            {
                let mut s = self.state.lock().unwrap();
                s.enabled = new_val;
            }
            let msg = InspectorMessage::Status { enabled: new_val };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = self.tx.send(json);
            }
        }

        /// Broadcast a widget tree snapshot to all connected clients.
        pub fn broadcast_tree(&self, root: Option<crate::types::WidgetNode>) {
            if !self.is_enabled() {
                return;
            }
            {
                let mut s = self.state.lock().unwrap();
                s.tree = root.clone();
            }
            let msg = InspectorMessage::Tree { root };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = self.tx.send(json);
            }
        }

        /// Broadcast the currently hovered widget ID.
        pub fn broadcast_hovered(&self, id: Option<u64>) {
            {
                let mut s = self.state.lock().unwrap();
                s.hovered_widget_id = id;
            }
            let msg = InspectorMessage::Hovered { id };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = self.tx.send(json);
            }
        }
    }

    pub struct InspectorServer;

    impl InspectorServer {
        /// Start the WebSocket inspector server on the given port.
        /// Returns an `InspectorHandle` that the CLI uses to read state and send commands.
        pub fn start(port: u16, runtime: &tokio::runtime::Handle) -> InspectorHandle {
            let (tx, _rx) = broadcast::channel::<String>(64);
            let enabled = Arc::new(AtomicBool::new(false));

            let state = Arc::new(Mutex::new(InspectorState::default()));
            let handle = InspectorHandle { enabled: enabled.clone(), tx: tx.clone(), state: state.clone() };

            let tx_server = tx.clone();
            let enabled_server = enabled.clone();
            let state_server = state.clone();
            runtime.spawn(async move {
                let addr = format!("127.0.0.1:{}", port);

                let listener = match tokio::net::TcpListener::bind(&addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        println!("[inspector] failed to bind server: {}", e);
                        return;
                    }
                };

                loop {
                    let (stream, _) = match listener.accept().await {
                        Ok(res) => res,
                        Err(_) => continue,
                    };

                    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                        Ok(ws) => ws,
                        Err(_) => continue,
                    };

                    {
                        let mut s = state_server.lock().unwrap();
                        s.connected = true;
                    }

                    let (mut write, mut read) = ws_stream.split();
                    let mut rx = tx_server.subscribe();

                    // Send current status immediately on connect
                    let status = InspectorMessage::Status { enabled: enabled_server.load(Ordering::Relaxed) };
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
                                                let mut s = state_server.lock().unwrap();
                                                s.tree = root;
                                            }
                                            InspectorMessage::Status { enabled } => {
                                                enabled_server.store(enabled, Ordering::Relaxed);
                                                let mut s = state_server.lock().unwrap();
                                                s.enabled = enabled;
                                            }
                                            InspectorMessage::Hovered { id } => {
                                                let mut s = state_server.lock().unwrap();
                                                s.hovered_widget_id = id;
                                            }
                                        }
                                    } else if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                                        if cmd.get("type").and_then(|v| v.as_str()) == Some("toggle") {
                                            let new_val = !enabled_server.load(Ordering::Relaxed);
                                            enabled_server.store(new_val, Ordering::Relaxed);
                                            let mut s = state_server.lock().unwrap();
                                            s.enabled = new_val;
                                            drop(s);
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

                    {
                        let mut s = state_server.lock().unwrap();
                        s.connected = false;
                    }
                    // println!("[inspector] disconnected from app");
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            });

            handle
        }

        /// Recursively walk the element tree and build a `WidgetNode` snapshot.
        pub fn snapshot_tree(element: &dyn Element) -> crate::types::WidgetNode {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            fn build(element: &dyn Element, counter: &AtomicU64) -> crate::types::WidgetNode {
                let (x, y, width, height) = if let Some((start, end)) = element.pos_start_end() {
                    (start.x, start.y, end.x - start.x, end.y - start.y)
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                };
                let id = counter.fetch_add(1, Ordering::Relaxed);
                let mut children = Vec::new();
                element.event_children(&mut |child| {
                    children.push(build(child, counter));
                });
                crate::types::WidgetNode {
                    id,
                    name: element.debug_name().to_string(),
                    element_type: std::any::type_name_of_val(element)
                        .rsplit("::")
                        .next()
                        .unwrap_or("Unknown")
                        .to_string(),
                    x, y, width, height, children,
                }
            }
            COUNTER.store(0, Ordering::Relaxed);
            build(element, &COUNTER)
        }
    }

    /// App-side inspector handle that connects to the CLI server as a WebSocket client.
    /// Used by the engine/app on all native targets.
    #[derive(Clone)]
    pub struct InspectorAppHandle {
        pub enabled: Arc<AtomicBool>,
        tx: tokio::sync::mpsc::UnboundedSender<String>,
    }

    impl InspectorAppHandle {
        /// Returns `true` if the inspector is currently active.
        pub fn is_enabled(&self) -> bool {
            self.enabled.load(Ordering::Relaxed)
        }

        /// Send a widget tree snapshot to the CLI server.
        pub fn broadcast_tree(&self, root: Option<crate::types::WidgetNode>) {
            if !self.is_enabled() {
                return;
            }
            let msg = InspectorMessage::Tree { root };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = self.tx.send(json);
            }
        }

        /// Send the currently hovered widget ID to the CLI server.
        pub fn broadcast_hovered(&self, id: Option<u64>) {
            let msg = InspectorMessage::Hovered { id };
            if let Ok(json) = serde_json::to_string(&msg) {
                let _ = self.tx.send(json);
            }
        }

        /// Connect to the CLI inspector server and return an `InspectorAppHandle`.
        pub fn connect(port: u16, runtime: &tokio::runtime::Handle) -> Self {
            let enabled = Arc::new(AtomicBool::new(false));
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

            let enabled_bg = enabled.clone();
            runtime.spawn(async move {
                let url = format!("ws://127.0.0.1:{}", port);

                loop {
                    let ws_stream = match tokio_tungstenite::connect_async(&url).await {
                        Ok((ws, _)) => ws,
                        Err(_) => {
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                    };

                    let (mut write, mut read) = ws_stream.split();

                    loop {
                        tokio::select! {
                            Some(msg_json) = rx.recv() => {
                                if write.send(Message::Text(Utf8Bytes::from(msg_json.as_str()))).await.is_err() {
                                    break;
                                }
                            }
                            incoming = read.next() => {
                                match incoming {
                                    Some(Ok(Message::Text(text))) => {
                                        if let Ok(msg) = serde_json::from_str::<InspectorMessage>(&text) {
                                            match msg {
                                                InspectorMessage::Status { enabled } => {
                                                    enabled_bg.store(enabled, Ordering::Relaxed);
                                                    widget::inspector_overlay::set_enabled(enabled);
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    Some(Ok(Message::Close(_))) | None => break,
                                    _ => {}
                                }
                            }
                        }
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            });

            InspectorAppHandle { enabled, tx }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod server {
    use crate::{InspectorMessage, WidgetNode};
    use serde::{Deserialize, Serialize};
    use std::cell::RefCell;
    use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{MessageEvent, WebSocket};
    use widget::Element;

    #[derive(Clone)]
    pub struct InspectorHandle {
        pub enabled: Arc<AtomicBool>,
        ws: Arc<RefCell<Option<WebSocket>>>,
    }

    impl InspectorHandle {
        pub fn is_enabled(&self) -> bool {
            self.enabled.load(Ordering::Relaxed)
        }

        pub fn set_enabled(&self, enabled: bool) {
            self.enabled.store(enabled, Ordering::Relaxed);
            widget::inspector_overlay::set_enabled(enabled);
            let msg = InspectorMessage::Status { enabled };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Some(ws) = self.ws.borrow().as_ref() {
                    if ws.ready_state() == 1 {
                        let _ = ws.send_with_str(&json);
                    }
                }
            }
        }

        pub fn broadcast_tree(&self, root: Option<WidgetNode>) {
            if !self.is_enabled() {
                return;
            }
            let msg = InspectorMessage::Tree { root };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Some(ws) = self.ws.borrow().as_ref() {
                    if ws.ready_state() == 1 { // WebSocket::OPEN
                        let _ = ws.send_with_str(&json);
                    }
                }
            }
        }

        pub fn broadcast_hovered(&self, id: Option<u64>) {
            let msg = InspectorMessage::Hovered { id };
            if let Ok(json) = serde_json::to_string(&msg) {
                if let Some(ws) = self.ws.borrow().as_ref() {
                    if ws.ready_state() == 1 {
                        let _ = ws.send_with_str(&json);
                    }
                }
            }
        }
    }

    pub fn start(port: u16) -> InspectorHandle {
        let enabled = Arc::new(AtomicBool::new(false));
        let ws_ref = Arc::new(RefCell::new(None));

        let url = format!("ws://127.0.0.1:{}", port);

        let ws = match WebSocket::new(&url) {
            Ok(ws) => ws,
            Err(_) => {
                return InspectorHandle { enabled, ws: ws_ref };
            }
        };

        *ws_ref.borrow_mut() = Some(ws.clone());

        let enabled_msg = enabled.clone();

        let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
            if let Some(txt) = e.data().as_string() {
                if let Ok(msg) = serde_json::from_str::<InspectorMessage>(&txt) {
                    match msg {
                        InspectorMessage::Status { enabled } => {
                            enabled_msg.store(enabled, Ordering::Relaxed);
                            widget::inspector_overlay::set_enabled(enabled);
                        }
                        _ => {}
                    }
                }
            }
        });

        ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
        onmessage_callback.forget();

        InspectorHandle { enabled, ws: ws_ref }
    }

    pub fn snapshot_tree(element: &dyn Element) -> WidgetNode {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        fn build(element: &dyn Element, counter: &AtomicU64) -> WidgetNode {
            let (x, y, width, height) = if let Some((start, end)) = element.pos_start_end() {
                (start.x as f32, start.y as f32, (end.x - start.x) as f32, (end.y - start.y) as f32)
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };
            let id = counter.fetch_add(1, Ordering::Relaxed);
            let mut children = Vec::new();
            element.event_children(&mut |child| {
                children.push(build(child, counter));
            });
            WidgetNode {
                id,
                name: element.debug_name().to_string(),
                element_type: std::any::type_name_of_val(element)
                    .rsplit("::")
                    .next()
                    .unwrap_or("Unknown")
                    .to_string(),
                x, y, width, height, children,
            }
        }
        COUNTER.store(0, Ordering::Relaxed);
        build(element, &COUNTER)
    }
}
