use std::cell::RefCell;
use std::rc::Rc;

use aimer_attribute::BoxConstraint;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_container::{Container, ZeroSizedBox};
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_macro::Rebuildable;
use aimer_space::Alignment;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, EventResult, LayoutElement,
    RequiredChild, VisitorElement, Widget,
};

use crate::ModalAnimation;
use crate::animation::visual_values;
use crate::host::{self, ModalHandle, ModalId, ModalTimeline};

/// Displays content above the entire application render tree.
///
/// Complete the builder with [`Modal::child`], then call [`Modal::show`] from a
/// callback to present it immediately through the framework-level overlay.
/// `AimerApp` installs the required host automatically.
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
///
/// use aimer_animation::Curve;
/// use aimer_container::SizedBox;
/// use aimer_modal::{Modal, ModalAnimation};
///
/// let handle =
///     Modal::new().animation(ModalAnimation::new().enter_duration(Duration::from_millis(200))
///                                                 .enter_curve(Curve::EaseOut))
///                 .child(SizedBox::new().width(320).height(180))
///                 .show();
///
/// handle.dismiss();
/// ```
pub struct Modal<W = RequiredChild> {
    child: W,
    barrier_color: Color,
    alignment: Alignment,
    animation: Option<ModalAnimation>,
    barrier_dismissible: bool,
    escape_dismissible: bool,
}

impl Default for Modal {
    fn default() -> Self {
        Self::new()
    }
}

impl Modal {
    /// Creates a centered modal with a 45%-opaque black barrier.
    pub fn new() -> Self {
        Self {
            child: RequiredChild,
            barrier_color: Color::BLACK.with_opacity(115),
            alignment: Alignment::MidCenter,
            animation: None,
            barrier_dismissible: true,
            escape_dismissible: true,
        }
    }

    /// Sets the viewport-wide barrier color.
    pub fn barrier_color(mut self, barrier_color: Color) -> Self {
        self.barrier_color = barrier_color;
        self
    }

    /// Sets the content alignment within the viewport.
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Enables a paint-only enter and exit transition.
    pub fn animation(mut self, animation: ModalAnimation) -> Self {
        self.animation = Some(animation);
        self
    }

    /// Controls whether pressing outside the content dismisses the modal.
    pub fn barrier_dismissible(mut self, dismissible: bool) -> Self {
        self.barrier_dismissible = dismissible;
        self
    }

    /// Controls whether a pressed Escape key dismisses the modal.
    pub fn escape_dismissible(mut self, dismissible: bool) -> Self {
        self.escape_dismissible = dismissible;
        self
    }

    /// Attaches the required modal content and completes this builder.
    pub fn child<W: Widget>(self, child: W) -> Modal<W> {
        Modal {
            child,
            barrier_color: self.barrier_color,
            alignment: self.alignment,
            animation: self.animation,
            barrier_dismissible: self.barrier_dismissible,
            escape_dismissible: self.escape_dismissible,
        }
    }

    /// Attaches and erases the required modal content.
    pub fn box_child<W: Widget + 'static>(self, child: W) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl<W: Widget + 'static> Modal<W> {
    /// Presents this modal through the application-wide host immediately.
    ///
    /// Calls made before the first application frame are queued and presented
    /// as soon as the root host is built.
    pub fn show(self) -> ModalHandle {
        let animation = self.animation;
        host::show(
            animation,
            Box::new(move |ctx, id, timeline| self.to_raw_element(ctx, Some(id), timeline)),
        )
    }

    fn to_raw_element(
        &self,
        ctx: &BuildContext,
        id: Option<ModalId>,
        timeline: Rc<RefCell<ModalTimeline>>,
    ) -> AnyElement {
        RawModal {
            barrier: Container::new()
                .color(self.barrier_color)
                .child(ZeroSizedBox)
                .to_element(ctx),
            child: self.child.to_element(ctx),
            alignment: self.alignment,
            animation: self.animation,
            timeline,
            id,
            barrier_dismissible: self.barrier_dismissible,
            escape_dismissible: self.escape_dismissible,
            child_bounds: RefCell::new(None),
        }
        .boxed()
    }

    #[cfg(test)]
    pub(crate) fn animation_config(&self) -> Option<ModalAnimation> {
        self.animation
    }

    #[cfg(test)]
    pub(crate) fn alignment_value(&self) -> Alignment {
        self.alignment
    }

    #[cfg(test)]
    pub(crate) fn barrier_color_value(&self) -> Color {
        self.barrier_color
    }
}

impl<W: Widget + 'static> Widget for Modal<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        self.to_raw_element(
            ctx,
            None,
            Rc::new(RefCell::new(ModalTimeline::new_static())),
        )
    }

    fn debug_name(&self) -> &'static str {
        "Modal"
    }
}

#[derive(Rebuildable)]
struct RawModal {
    barrier: AnyElement,
    child: AnyElement,
    alignment: Alignment,
    animation: Option<ModalAnimation>,
    timeline: Rc<RefCell<ModalTimeline>>,
    id: Option<ModalId>,
    barrier_dismissible: bool,
    escape_dismissible: bool,
    child_bounds: RefCell<Option<(Vec2d, Vec2d)>>,
}

impl Drawable for RawModal {
    fn draw(&self, ctx: &BuildContext) {
        let progress = self.timeline.borrow().progress();
        let scale_from = self
            .animation
            .map(|animation| animation.content_scale_from)
            .unwrap_or(1.0);
        let (opacity, scale) = visual_values(progress, scale_from);

        ctx.canvas.set_alpha(opacity);
        self.barrier.draw(ctx);
        ctx.canvas.restore_alpha();

        let child_size = self.child.computed_size(ctx);
        let (offset_x, offset_y) = alignment_offset(self.alignment, ctx.parent_size, child_size);
        let origin = Vec2d {
            x: ctx.parent_pos.x + offset_x,
            y: ctx.parent_pos.y + offset_y,
        };
        *self.child_bounds.borrow_mut() = Some((
            origin,
            Vec2d {
                x: origin.x + child_size.width,
                y: origin.y + child_size.height,
            },
        ));

        let mut child_ctx = ctx.clone();
        child_ctx.parent_size = child_size;
        child_ctx.parent_pos = origin;
        child_ctx.box_constraint = BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: child_size.width,
            max_height: child_size.height,
        };
        child_ctx.visible_rect = ctx
            .visible_rect
            .map(|(x, y, width, height)| (x - offset_x, y - offset_y, width, height));

        let center = Vec2d {
            x: offset_x + child_size.width / 2.0,
            y: offset_y + child_size.height / 2.0,
        };
        ctx.canvas.save();
        ctx.canvas.translate(center);
        ctx.canvas.scale(scale, scale);
        ctx.canvas.translate(Vec2d {
            x: -child_size.width / 2.0,
            y: -child_size.height / 2.0,
        });
        ctx.canvas.set_alpha(opacity);
        self.child.draw(&child_ctx);
        ctx.canvas.restore_alpha();
        ctx.canvas.restore();
    }
}

impl EventElement for RawModal {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        let dismiss = match event {
            ElementEvent::PointerDown(position, _, _)
                if self.barrier_dismissible && !self.contains_child(*position) =>
            {
                true
            }
            ElementEvent::KeyInput {
                key: NamedKey::Escape,
                action: KeyAction::Pressed,
                ..
            } if self.escape_dismissible => true,
            _ => false,
        };
        if dismiss && let Some(id) = self.id {
            host::dismiss(id);
        }
        EventResult::consumed()
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.barrier.as_ref());
        visitor(self.child.as_ref());
    }
}

impl RawModal {
    fn contains_child(&self, position: Vec2d) -> bool {
        self.child_bounds.borrow().is_some_and(|(start, end)| {
            position.x >= start.x
                && position.x <= end.x
                && position.y >= start.y
                && position.y <= end.y
        })
    }
}

impl LayoutElement for RawModal {
    fn size(&self) -> Option<Size> {
        None
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ctx.parent_size
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ctx.parent_size
    }
}

impl VisitorElement for RawModal {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.barrier.as_ref());
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "Modal"
    }
}

fn alignment_offset(alignment: Alignment, parent: ResolvedSize, child: ResolvedSize) -> (f32, f32) {
    let remaining_width = (parent.width - child.width).max(0.0);
    let remaining_height = (parent.height - child.height).max(0.0);
    let x = match alignment {
        Alignment::TopLeft | Alignment::MidLeft | Alignment::BotLeft => 0.0,
        Alignment::TopCenter | Alignment::MidCenter | Alignment::BotCenter => remaining_width / 2.0,
        Alignment::TopRight | Alignment::MidRight | Alignment::BotRight => remaining_width,
    };
    let y = match alignment {
        Alignment::TopLeft | Alignment::TopCenter | Alignment::TopRight => 0.0,
        Alignment::MidLeft | Alignment::MidCenter | Alignment::MidRight => remaining_height / 2.0,
        Alignment::BotLeft | Alignment::BotCenter | Alignment::BotRight => remaining_height,
    };
    (x, y)
}
