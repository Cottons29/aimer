//! Shared helpers for the flex unit tests.
//!
//! Flex layout needs a [`BuildContext`], which in turn needs a canvas, a window
//! handle, and (off the web) an async runtime handle. Building one by hand in
//! every test module is noisy, so the headless construction lives here together
//! with the probe elements used to observe how many children a pass touches.

use std::cell::Cell;
use std::rc::Rc;

use aimer_attribute::BoxConstraint;
use aimer_attribute::size::ResolvedSize;
use aimer_canvas::{Canvas, InnerCanvas};
use aimer_widget::base::{BuildContext, WindowHandle};
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement,
};

#[cfg(not(target_arch = "wasm32"))]
fn dummy_async_handle() -> tokio::runtime::Handle {
    use std::sync::OnceLock;

    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
    });
    let _guard = runtime.enter();
    tokio::runtime::Handle::current()
}

/// Builds a headless [`BuildContext`] constrained to `width` x `height`.
///
/// `visible_rect` is passed through verbatim so a test can emulate the viewport
/// a `Scrollable` hands to its child.
pub(crate) fn dummy_build_context(
    width: f32,
    height: f32,
    visible_rect: Option<(f32, f32, f32, f32)>,
) -> BuildContext<'static> {
    let canvas = {
        let leaked: &'static InnerCanvas = Box::leak(Box::new(InnerCanvas::new()));
        Canvas::new(leaked)
    };

    BuildContext {
        parent_size: ResolvedSize { width, height },
        canvas,
        scale: 1.0,
        parent_pos: Default::default(),
        cursor_pos: Default::default(),
        box_constraint: BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: width,
            max_height: height,
        },
        visible_rect,
        window: WindowHandle::headless(
            winit::dpi::PhysicalSize::new(width.max(1.0) as u32, height.max(1.0) as u32),
            1.0,
        ),
        #[cfg(not(target_arch = "wasm32"))]
        async_handle: dummy_async_handle(),
        inherited_states: Default::default(),
    }
}

/// A leaf element with a fixed size that counts how often it is measured and
/// drawn.
///
/// Both counters are shared, so a test can total the work an entire child list
/// received during one pass.
pub(crate) struct CountingChild {
    size: ResolvedSize,
    measured: Rc<Cell<usize>>,
    drawn: Rc<Cell<usize>>,
}

impl CountingChild {
    /// Creates an erased counting child of `width` x `height`.
    pub(crate) fn boxed_new(
        width: f32,
        height: f32,
        measured: &Rc<Cell<usize>>,
        drawn: &Rc<Cell<usize>>,
    ) -> AnyElement {
        Self {
            size: ResolvedSize { width, height },
            measured: measured.clone(),
            drawn: drawn.clone(),
        }
        .boxed()
    }
}

impl VisitorElement for CountingChild {
    fn debug_name(&self) -> &'static str {
        "CountingChild"
    }
}

impl EventElement for CountingChild {}

impl Rebuildable for CountingChild {}

impl Drawable for CountingChild {
    fn draw(&self, _ctx: &BuildContext) {
        self.drawn.set(self.drawn.get() + 1);
    }
}

impl LayoutElement for CountingChild {
    fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
        self.measured.set(self.measured.get() + 1);
        self.size
    }
}

/// A leaf element whose main-axis extent can change between frames, standing in
/// for an implicitly animated child.
///
/// It records the canvas translation it was drawn at, so a test can assert that
/// its siblings moved with it.
pub(crate) struct ResizingChild {
    height: Rc<Cell<f32>>,
    drawn_at: Rc<Cell<(f32, f32)>>,
}

impl ResizingChild {
    /// Creates an erased child whose height follows `height`.
    pub(crate) fn boxed_new(height: &Rc<Cell<f32>>, drawn_at: &Rc<Cell<(f32, f32)>>) -> AnyElement {
        Self {
            height: height.clone(),
            drawn_at: drawn_at.clone(),
        }
        .boxed()
    }
}

impl VisitorElement for ResizingChild {
    fn debug_name(&self) -> &'static str {
        "ResizingChild"
    }
}

impl EventElement for ResizingChild {}

impl Rebuildable for ResizingChild {}

impl Drawable for ResizingChild {
    fn draw(&self, ctx: &BuildContext) {
        let translation = ctx.canvas.get_transform_translation();
        self.drawn_at.set((translation.0, translation.1));
    }
}

impl LayoutElement for ResizingChild {
    fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
        ResolvedSize {
            width: 10.0,
            height: self.height.get(),
        }
    }
}
