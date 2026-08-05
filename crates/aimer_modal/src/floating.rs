use std::cell::RefCell;
use std::rc::Rc;

use aimer_attribute::bounds::Bounds;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_container::{Container, ZeroSizedBox};
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_macro::Rebuildable;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, EventResult, LayoutElement,
    RequiredChild, VisitorElement, Widget,
};

use crate::anchor::AnchorHandle;
use crate::animation::visual_values;
use crate::host::{self, ModalHandle, ModalId, ModalTimeline};
use crate::paint::{contains, paint_overlay_content};
use crate::placement::{FloatingAlign, FloatingSide, OverflowPolicy, PlacementSpec};
use crate::{ModalAnimation, resolve_placement};

/// Displays content pinned to an [`crate::Anchor`] above the entire
/// application render tree.
///
/// `Floating` is the anchored placement primitive of the framework: it reuses
/// the same overlay entry stack as [`crate::Modal`] — and therefore the same
/// stacking order, pointer routing, `Escape` handling and enter/exit
/// animation — but resolves its position from the anchor rectangle instead of
/// the viewport. Dropdown menus, tooltips, selects and context menus are meant
/// to be composed from it rather than to reimplement it.
///
/// The panel is re-positioned on every frame, so it keeps following its anchor
/// while the anchor moves, resizes or scrolls. When the resolved position would
/// leave the viewport, the configured [`OverflowPolicy`] flips the panel to the
/// opposite side or slides it back inside.
///
/// Unlike a modal, the barrier defaults to [`Color::Transparent`]: the content
/// below stays visible while pointer presses outside the panel still dismiss
/// it.
///
/// # Example
///
/// ```rust
/// use aimer_container::SizedBox;
/// use aimer_modal::{Anchor, AnchorHandle, Floating, FloatingAlign, FloatingSide};
///
/// let anchor = AnchorHandle::new();
/// let _trigger = Anchor::new().handle(anchor.clone())
///                             .child(SizedBox::new().width(120).height(32));
///
/// let handle = Floating::new().anchor(anchor)
///                             .side(FloatingSide::Bottom)
///                             .align(FloatingAlign::Start)
///                             .gap(4.0)
///                             .child(SizedBox::new().width(180).height(240))
///                             .show();
///
/// handle.dismiss();
/// ```
pub struct Floating<W = RequiredChild> {
    child: W,
    anchor: AnchorHandle,
    placement: PlacementSpec,
    barrier_color: Color,
    animation: Option<ModalAnimation>,
    barrier_dismissible: bool,
    escape_dismissible: bool,
}

impl Default for Floating {
    fn default() -> Self {
        Self::new()
    }
}

impl Floating {
    /// Creates a panel placed below an untracked anchor, with a transparent
    /// barrier that dismisses on an outside press or `Escape`.
    #[inline]
    pub fn new() -> Self {
        Self {
            child: RequiredChild,
            anchor: AnchorHandle::new(),
            placement: PlacementSpec::new(),
            barrier_color: Color::Transparent,
            animation: None,
            barrier_dismissible: true,
            escape_dismissible: true,
        }
    }

    /// Pins the panel to the rectangle reported by `anchor`.
    #[inline]
    pub fn anchor(mut self, anchor: AnchorHandle) -> Self {
        self.anchor = anchor;
        self
    }

    /// Replaces the whole placement request at once.
    #[inline]
    pub fn placement(mut self, placement: PlacementSpec) -> Self {
        self.placement = placement;
        self
    }

    /// Sets the preferred side of the anchor.
    #[inline]
    pub fn side(mut self, side: FloatingSide) -> Self {
        self.placement = self.placement.side(side);
        self
    }

    /// Sets the cross-axis alignment against the anchor.
    #[inline]
    pub fn align(mut self, align: FloatingAlign) -> Self {
        self.placement = self.placement.align(align);
        self
    }

    /// Sets the distance kept between the anchor and the panel.
    #[inline]
    pub fn gap(mut self, gap: f32) -> Self {
        self.placement = self.placement.gap(gap);
        self
    }

    /// Sets an additional translation applied to the resolved position.
    #[inline]
    pub fn offset(mut self, offset: Vec2d) -> Self {
        self.placement = self.placement.offset(offset);
        self
    }

    /// Sets what happens when the panel does not fit inside the viewport.
    #[inline]
    pub fn overflow(mut self, overflow: OverflowPolicy) -> Self {
        self.placement = self.placement.overflow(overflow);
        self
    }

    /// Sets the viewport-wide barrier color.
    #[inline]
    pub fn barrier_color(mut self, barrier_color: Color) -> Self {
        self.barrier_color = barrier_color;
        self
    }

    /// Enables a paint-only enter and exit transition.
    #[inline]
    pub fn animation(mut self, animation: ModalAnimation) -> Self {
        self.animation = Some(animation);
        self
    }

    /// Controls whether pressing outside the panel dismisses it.
    #[inline]
    pub fn barrier_dismissible(mut self, dismissible: bool) -> Self {
        self.barrier_dismissible = dismissible;
        self
    }

    /// Controls whether a pressed Escape key dismisses the panel.
    #[inline]
    pub fn escape_dismissible(mut self, dismissible: bool) -> Self {
        self.escape_dismissible = dismissible;
        self
    }

    /// Attaches the required panel content and completes this builder.
    #[inline]
    pub fn child<W: Widget>(self, child: W) -> Floating<W> {
        Floating {
            child,
            anchor: self.anchor,
            placement: self.placement,
            barrier_color: self.barrier_color,
            animation: self.animation,
            barrier_dismissible: self.barrier_dismissible,
            escape_dismissible: self.escape_dismissible,
        }
    }

    /// Attaches and erases the required panel content.
    #[inline]
    pub fn box_child<W: Widget + 'static>(self, child: W) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl<W: Widget + 'static> Floating<W> {
    /// Presents this panel through the application-wide host immediately.
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
        RawFloating {
            barrier: Container::new()
                .color(self.barrier_color)
                .child(ZeroSizedBox)
                .to_element(ctx),
            child: self.child.to_element(ctx),
            anchor: self.anchor.clone(),
            placement: self.placement,
            animation: self.animation,
            timeline,
            id,
            barrier_dismissible: self.barrier_dismissible,
            escape_dismissible: self.escape_dismissible,
            child_bounds: RefCell::new(None),
        }
        .boxed()
    }

    /// Returns the handle this panel is pinned to.
    #[inline]
    pub fn anchor_value(&self) -> AnchorHandle {
        self.anchor.clone()
    }

    /// Returns the placement request resolved on every frame.
    #[inline]
    pub fn placement_value(&self) -> PlacementSpec {
        self.placement
    }

    /// Returns the barrier color painted below the panel.
    #[inline]
    pub fn barrier_color_value(&self) -> Color {
        self.barrier_color
    }

    #[cfg(test)]
    pub(crate) fn animation_config(&self) -> Option<ModalAnimation> {
        self.animation
    }
}

impl<W: Widget + 'static> Widget for Floating<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        self.to_raw_element(
            ctx,
            None,
            Rc::new(RefCell::new(ModalTimeline::new_static())),
        )
    }

    fn debug_name(&self) -> &'static str {
        "Floating"
    }
}

#[derive(Rebuildable)]
struct RawFloating {
    barrier: AnyElement,
    child: AnyElement,
    anchor: AnchorHandle,
    placement: PlacementSpec,
    animation: Option<ModalAnimation>,
    timeline: Rc<RefCell<ModalTimeline>>,
    id: Option<ModalId>,
    barrier_dismissible: bool,
    escape_dismissible: bool,
    child_bounds: RefCell<Option<(Vec2d, Vec2d)>>,
}

impl RawFloating {
    /// Returns the anchor rectangle in the painted coordinate space.
    ///
    /// Anchors record logical units, while the paint pass works in scaled
    /// units. An anchor that has never been painted collapses to the viewport
    /// origin, which places the panel in the top-left corner instead of an
    /// arbitrary position.
    fn anchor_bounds(&self, ctx: &BuildContext) -> Bounds {
        let scale = if ctx.scale > 0.0 { ctx.scale } else { 1.0 };
        self.anchor
            .bounds()
            .map(|bounds| bounds * scale)
            .unwrap_or_else(|| Bounds::new(ctx.parent_pos.x, ctx.parent_pos.y, 0.0, 0.0))
    }
}

impl Drawable for RawFloating {
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
        let placement = resolve_placement(
            self.placement,
            self.anchor_bounds(ctx),
            child_size,
            ctx.parent_size,
        );
        paint_overlay_content(
            ctx,
            &self.child,
            child_size,
            Vec2d {
                x: placement.origin.x - ctx.parent_pos.x,
                y: placement.origin.y - ctx.parent_pos.y,
            },
            (opacity, scale),
            &self.child_bounds,
        );
    }
}

impl EventElement for RawFloating {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        let dismiss = match event {
            ElementEvent::PointerDown(pointer)
                if self.barrier_dismissible && !contains(&self.child_bounds, pointer.pos) =>
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

impl LayoutElement for RawFloating {
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

impl VisitorElement for RawFloating {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.barrier.as_ref());
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "Floating"
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use aimer_animation::Curve;
    use aimer_container::ZeroSizedBox;

    use super::*;
    use crate::anchor::Anchor;

    #[test]
    fn a_new_panel_hangs_below_its_anchor_behind_a_transparent_barrier() {
        let floating = Floating::new().child(ZeroSizedBox);

        assert_eq!(floating.barrier_color_value(), Color::Transparent);
        assert_eq!(
            floating.placement_value().side_value(),
            FloatingSide::Bottom
        );
        assert_eq!(
            floating.placement_value().align_value(),
            FloatingAlign::Start
        );
        assert_eq!(
            floating.placement_value().overflow_value(),
            OverflowPolicy::Flip
        );
    }

    #[test]
    fn configuration_survives_child_attachment() {
        let anchor = Anchor::new().child(ZeroSizedBox).handle_value();
        let animation = ModalAnimation::new()
            .enter_duration(Duration::from_millis(120))
            .enter_curve(Curve::EaseOut);
        let floating = Floating::new()
            .anchor(anchor.clone())
            .side(FloatingSide::Right)
            .align(FloatingAlign::Center)
            .gap(6.0)
            .offset(Vec2d { x: 2.0, y: -3.0 })
            .overflow(OverflowPolicy::Shift)
            .barrier_color(Color::BLACK.with_opacity(40))
            .animation(animation)
            .child(ZeroSizedBox);

        assert_eq!(floating.anchor_value(), anchor);
        assert_eq!(floating.placement_value().side_value(), FloatingSide::Right);
        assert_eq!(
            floating.placement_value().align_value(),
            FloatingAlign::Center
        );
        assert_eq!(floating.placement_value().gap_value(), 6.0);
        assert_eq!(
            floating.placement_value().offset_value(),
            Vec2d { x: 2.0, y: -3.0 }
        );
        assert_eq!(
            floating.placement_value().overflow_value(),
            OverflowPolicy::Shift
        );
        assert_eq!(
            floating.barrier_color_value(),
            Color::BLACK.with_opacity(40)
        );
        assert_eq!(floating.animation_config(), Some(animation));
    }

    #[test]
    fn a_whole_placement_spec_can_replace_the_individual_setters() {
        let spec = PlacementSpec::new()
            .side(FloatingSide::Top)
            .overflow(OverflowPolicy::Fixed);
        let floating = Floating::new().placement(spec).child(ZeroSizedBox);

        assert_eq!(floating.placement_value(), spec);
    }

    #[test]
    fn show_and_dismiss_enqueue_framework_commands_immediately() {
        host::reset_registry_for_test();

        let handle = Floating::new().child(ZeroSizedBox).show();
        assert_eq!(host::pending_command_count_for_test(), 1);

        assert!(handle.dismiss());
        assert!(!handle.dismiss());
        assert_eq!(host::pending_command_count_for_test(), 2);
    }
}
