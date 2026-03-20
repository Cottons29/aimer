pub mod event_handler;
mod user_events;

#[allow(unused)]
use crate::handler;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use std::cell::Cell;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use widget::base::BuildContext;
use widget::{Element, Widget};
use winit::application::ApplicationHandler;
#[allow(unused)]
use winit::dpi::{LogicalSize, PhysicalSize, Position};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
#[allow(unused)]
use winit::monitor::MonitorHandle;
#[allow(unused)]
use winit::window::{self, Fullscreen, Window, WindowAttributes, WindowId};


use crate::render_ctx::AimerRenderContext;
use inspector::InspectorOverlay;
#[cfg(not(target_arch = "wasm32"))]
use inspector::InspectorServer;
use utils::debug;
use crate::handler::event_handler::WindowEventHandler;
use crate::handler::user_events::handle_user_event;

#[cfg(target_arch = "wasm32")]
pub(crate) type Float = f64;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type Float = f32;

/// Walk the snapshot tree and find a node matching the hovered widget by name and bounds.
#[cfg(debug_assertions)]
fn find_hovered_node(node: &inspector::WidgetNode, name: &str, start: Vec2d, end: Vec2d) -> Option<u64> {
    const EPS: f32 = 1.0;
    let w = (end.x - start.x) as f32;
    let h = (end.y - start.y) as f32;
    if node.name == name
        && (node.x - start.x as f32).abs() < EPS
        && (node.y - start.y as f32).abs() < EPS
        && (node.width - w).abs() < EPS
        && (node.height - h).abs() < EPS
    {
        return Some(node.id);
    }
    for child in &node.children {
        if let Some(id) = find_hovered_node(child, name, start, end) {
            return Some(id);
        }
    }
    None
}

pub struct AimerApplicationHandler {
    pub window: Option<&'static Window>,
    pub render_ctx: AimerRenderContext,
    pub widget_root: Option<Box<dyn Element>>,
    pub pending_widget: Option<Box<dyn Widget>>,
    pub cursor_pos: Vec2d,
    pub current_modifiers: events::element::Modifiers,
    pub window_scale: f64,
    pub native_window_size: Option<ResolvedSize>,
    pub pending_resize: Option<PhysicalSize<u32>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_runtime: Runtime,
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub inspector: inspector::InspectorAppHandle,
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    pub inspector: inspector::InspectorHandle,
    #[cfg(debug_assertions)]
    pub inspector_change: Cell<bool>,
    #[cfg(debug_assertions)]
    pub inspector_prev_enabled: Cell<bool>,
    #[cfg(debug_assertions)]
    pub inspector_redraw_frames: Cell<u8>,
}

impl ApplicationHandler<crate::aimer_app::AimerCustomAppEvent> for AimerApplicationHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        {
            match crate::ios_screen::get_screen_resolution_pixels() {
                Some((width, height)) => {
                    self.native_window_size = Some(ResolvedSize { width: width as f32, height: height as f32 })
                }
                None => (),
            };
        }

        let window_attributes = {
            #[cfg(not(target_os = "ios"))]
            {
                WindowAttributes::default()
            }
            #[cfg(target_os = "ios")]
            {
                match crate::ios_screen::get_screen_resolution_pixels() {
                    Some((w, h)) => {
                        // println!("IOS TARGET Window Size : {w}x{h}");
                        let phy_size = PhysicalSize::new(w as u32, h as u32);
                        WindowAttributes::default().with_inner_size(phy_size)
                    }
                    None => WindowAttributes::default(),
                }
            }
        };

        if self.window.is_none() {
            let window = event_loop.create_window(window_attributes).unwrap();
            let window: &'static Window = Box::leak(Box::new(window)); // Leak to static ref
            events::window::set_window(window);
            self.window = Some(window);
        }




        let window = self.window.unwrap();
        let size = window.inner_size();

        debug!("Window Size : {size:?}");


        self.render_ctx.initialize(window, size);

        self.window_scale = window.scale_factor();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: crate::aimer_app::AimerCustomAppEvent) {
        debug!("User event {:?}", event);
        handle_user_event(self, event);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        WindowEventHandler::handle_events(self, event_loop, _id, event);
    }

    #[cfg(debug_assertions)]
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let current = self.inspector.is_enabled();
        let prev = self.inspector_prev_enabled.get();
        if current != prev {
            self.inspector_prev_enabled.set(current);
            self.inspector_change.set(true);
            self.inspector_redraw_frames.set(5);
        }
        let frames = self.inspector_redraw_frames.get();
        if frames > 0 {
            self.inspector_redraw_frames.set(frames - 1);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}
#[allow(dead_code)]
impl AimerApplicationHandler {
    fn render_widget_tree(widget: &dyn Element, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        if let Ok(mut hovered) = widget::inspector_overlay::HOVERED_WIDGET.write() {
            *hovered = None;
        }

        ctx.canvas.save();
        widget.draw(ctx);
        ctx.canvas.restore();
    }

    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    fn broadcast_inspector_snapshot(&self) {
        if self.inspector.is_enabled() {
            let snapshot = self
                .widget_root
                .as_ref()
                .map(|root| InspectorServer::snapshot_tree(root.as_ref()));

            let hovered_id = if let Ok(hovered) = widget::inspector_overlay::HOVERED_WIDGET.read() {
                if let Some((name, start, end)) = hovered.as_ref() {
                    snapshot
                        .as_ref()
                        .and_then(|s| find_hovered_node(s, name, *start, *end))
                } else {
                    None
                }
            } else {
                None
            };

            self.inspector.broadcast_tree(snapshot);
            self.inspector.broadcast_hovered(hovered_id);
        }
    }

    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    fn broadcast_inspector_snapshot(&self) {
        if self.inspector.is_enabled() {
            let snapshot = self
                .widget_root
                .as_ref()
                .map(|root| inspector::snapshot_tree(root.as_ref()));

            let hovered_id = if let Ok(hovered) = widget::inspector_overlay::HOVERED_WIDGET.read() {
                if let Some((name, start, end)) = hovered.as_ref() {
                    snapshot
                        .as_ref()
                        .and_then(|s| find_hovered_node(s, name, *start, *end))
                } else {
                    None
                }
            } else {
                None
            };

            self.inspector.broadcast_tree(snapshot);
            self.inspector.broadcast_hovered(hovered_id);
        }
    }

    #[allow(unused)]
    pub(crate) fn render(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(debug_assertions)]
        {
            let current = self.inspector.is_enabled();
            let prev = self.inspector_prev_enabled.get();
            if current != prev {
                self.inspector_prev_enabled.set(current);
                self.inspector_change.set(true);
                self.inspector_redraw_frames.set(5);
            }
            let frames = self.inspector_redraw_frames.get();
            if frames > 0 {
                self.inspector_redraw_frames.set(frames - 1);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }

        #[allow(clippy::collapsible_if)]
        if let Some(size) = self.pending_resize.take() {
            #[cfg(target_arch = "wasm32")]
            if let Some(window) = &self.window {
                self.render_ctx.resize(window, size);
            }
            #[cfg(not(target_arch = "wasm32"))]
            self.render_ctx.resize(size);
        }

        let Some(window) = self.window else { return };
        let window_scale = self.window_scale;
        let cursor_pos = self.cursor_pos;
        #[cfg(not(target_arch = "wasm32"))]
        let async_handle = self.async_runtime.handle().clone();
        let widget_root = &mut self.widget_root;
        let pending_widget = &mut self.pending_widget;
        #[cfg(debug_assertions)]
        let inspector_enabled = self.inspector.is_enabled();

        let draw_widgets = |canvas: &_, width: u32, height: u32| {
            let build_ctx = BuildContext {
                parent_size: ResolvedSize { width: width as Float, height: height as Float },
                canvas,
                scale: window_scale as Float,
                parent_pos: Default::default(),
                cursor_pos,
                box_constraint: widget::style::BoxConstraint {
                    min_width: 0.0,
                    min_height: 0.0,
                    max_width: width as Float,
                    max_height: height as Float,
                },
                visible_rect: None,
                window,
                #[cfg(not(target_arch = "wasm32"))]
                async_handle: async_handle.clone(),
                inherited_states: Default::default(),
            };

            #[allow(clippy::collapsible_if)]
            if widget_root.is_none() {
                if let Some(w) = pending_widget.take() {
                    *widget_root = Some(w.to_element(&build_ctx));
                }
            }

            if let Some(root) = widget_root {
                Self::render_widget_tree(root.as_ref(), &build_ctx);
                #[cfg(debug_assertions)]
                if inspector_enabled {
                    InspectorOverlay::draw(root.as_ref(), build_ctx.canvas, cursor_pos, build_ctx.scale as f32);
                }
            }
        };

        #[cfg(not(target_arch = "wasm32"))]
        self.render_ctx.render_frame(
            #[cfg(target_os = "android")]
            window,
            draw_widgets,
        );

        #[cfg(target_arch = "wasm32")]
        self.render_ctx.render_frame(window, draw_widgets);

        #[cfg(debug_assertions)]
        self.broadcast_inspector_snapshot();
    }
}

#[cfg(test)]
mod tests {
    use attribute::position::Vec2d;
    use attribute::size::Size;
    use widget::base::BuildContext;
    use widget::{Drawable, Element};

    #[allow(dead_code)]
    struct MockWidget {
        pos: Option<Vec2d>,
        size: Option<Size>,
        children: Vec<Box<dyn Element>>,
    }

    impl Drawable for MockWidget {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Element for MockWidget {
        fn pos(&self) -> Option<Vec2d> {
            self.pos
        }
        fn size(&self) -> Option<Size> {
            self.size
        }
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }
    }
}
