//! The geometry of a container transform: a whole page growing out of an item.
//!
//! Tapping an item in a grid makes the destination page start scaled down
//! inside that item, with rounded corners, and grow to fill the viewport while
//! the page behind it dims. Going back plays the same motion in reverse into
//! the same item. This module is only the geometry; hosts such as the router
//! apply it to retained pages.

use aimer_attribute::size::ResolvedSize;
use aimer_canvas::Mat3;

use super::animated_paint::PaintPresentation;
use crate::primitives::curve::{AnyCurve, Curve};

/// A rectangle in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ElementBounds {
    /// Creates bounds from the top-left corner and size.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Whether the bounds are finite and enclose a positive area.
    pub fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }
}

/// Presentations of the layers of a container transform at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContainerTransformFrame {
    /// The page that grows out of the item (or shrinks back into it).
    pub page: PaintPresentation,
    /// The page behind it, which dims while the page is on top.
    pub backdrop: PaintPresentation,
    /// A copy of the item itself, drawn above everything else.
    ///
    /// It leads the motion: the item scales up to the page's card first, then
    /// dissolves into the page. Fully transparent unless the spec is
    /// [`anchored`](ContainerTransformSpec::anchored). The copy is laid out at
    /// the item's own size with its top-left at the host's origin.
    pub anchor: PaintPresentation,
}

/// Describes a page growing out of one item.
///
/// The page is scaled uniformly so its width matches the item's, which keeps
/// its own proportions: the card is as tall as the page is, and may overlap the
/// items below the one it grew from. Scale, position and corner radius then
/// interpolate to the full viewport.
///
/// With an [`anchor`](Self::anchored) the first part of the motion belongs to
/// the item rather than the page. For the first quarter of the timeline (the
/// [`handoff`](Self::handoff)) a copy of the item scales up to the card's size
/// while the page stays hidden; around the handoff it dissolves into the page,
/// which then carries on to fill the viewport. Collapsing plays the same thing
/// in reverse, so the page shrinks, dissolves into the item, and the item
/// scales back into its slot.
///
/// `sample` takes the progress of the *expansion*: `0.0` is the page sitting on
/// the item and `1.0` is the page filling the viewport. Play it backwards to
/// collapse.
///
/// # Example
///
/// ```rust
/// use aimer_animation::{ContainerTransformSpec, ElementBounds};
/// use aimer_attribute::size::ResolvedSize;
///
/// let viewport = ResolvedSize { width: 400.0, height: 800.0 };
/// let item = ElementBounds::new(20.0, 120.0, 160.0, 160.0);
/// let spec = ContainerTransformSpec::new(item, viewport).unwrap();
///
/// // On the item at first, filling the screen at the end.
/// let start = spec.sample(0.0).page;
/// assert_eq!(start.transform.transform_point(0.0, 0.0), (20.0, 120.0));
/// assert_eq!(spec.sample(1.0).page.transform.transform_point(0.0, 0.0), (0.0, 0.0));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerTransformSpec {
    source: ElementBounds,
    viewport: ResolvedSize,
    source_radius: f32,
    page_radius: f32,
    scrim: f32,
    anchored: bool,
    handoff: f32,
    blend: f32,
    anchor_curve: AnyCurve,
    page_curve: AnyCurve,
}

/// Where the page is fully opaque, as a fraction of the expansion, when there is
/// no anchor to hand off from. Fading in early hides the moment the page
/// replaces the item it grows from.
const FADE_IN_END: f32 = 0.25;

/// Where the item hands over to the page, as a fraction of the expansion.
const DEFAULT_HANDOFF: f32 = 0.25;

/// How much of the expansion the item-to-page dissolve spans, centred on the
/// handoff.
const DEFAULT_BLEND: f32 = 0.1;

impl ContainerTransformSpec {
    /// Describes a page growing from `source` to fill `viewport`.
    ///
    /// Both are in the same logical-pixel space, with `source` measured from the
    /// viewport's top-left corner. Returns `None` if either has no positive,
    /// finite area.
    pub fn new(source: ElementBounds, viewport: ResolvedSize) -> Option<Self> {
        let viewport_ok = viewport.width.is_finite()
            && viewport.height.is_finite()
            && viewport.width > 0.0
            && viewport.height > 0.0;
        (source.is_valid() && viewport_ok).then_some(Self {
            source,
            viewport,
            source_radius: 12.0,
            page_radius: 0.0,
            scrim: 0.35,
            anchored: false,
            handoff: DEFAULT_HANDOFF,
            blend: DEFAULT_BLEND,
            anchor_curve: AnyCurve::default(),
            page_curve: AnyCurve::default(),
        })
    }

    /// Sets the corner radius of the page while it sits on the item, as shown.
    /// The default is 12 logical pixels. Negative or non-finite values are
    /// treated as zero.
    pub fn source_radius(mut self, radius: f32) -> Self {
        self.source_radius = sanitize(radius);
        self
    }

    /// Sets the corner radius of the page once it fills the viewport. The
    /// default is zero. A non-zero radius ends with square corners when the
    /// host removes the clip, so give the page the same radius itself.
    pub fn page_radius(mut self, radius: f32) -> Self {
        self.page_radius = sanitize(radius);
        self
    }

    /// Sets how much the page behind fades by the time the page is on top, in
    /// `0.0..=1.0`. The default is `0.35`.
    pub fn scrim(mut self, scrim: f32) -> Self {
        self.scrim = if scrim.is_finite() {
            scrim.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }

    /// Draws a copy of the item over the pages, leading the motion.
    ///
    /// Without it the page alone grows out of the item, fading in over the first
    /// quarter. Enable it only when the item can actually be drawn again; see
    /// [`ContainerTransformFrame::anchor`].
    pub fn anchored(mut self, anchored: bool) -> Self {
        self.anchored = anchored;
        self
    }

    /// Sets where the item hands over to the page, as a fraction of the
    /// expansion in `0.0..=1.0`. The default is `0.25`. A non-finite value is
    /// ignored.
    pub fn handoff(mut self, handoff: f32) -> Self {
        self.handoff = if handoff.is_finite() { handoff.clamp(0.0, 1.0) } else { DEFAULT_HANDOFF };
        self
    }

    /// Sets how much of the expansion the item-to-page dissolve spans, centred
    /// on the handoff, in `0.0..=1.0`. Zero swaps them at the handoff. The
    /// default is `0.1`. A non-finite value is ignored.
    pub fn blend(mut self, blend: f32) -> Self {
        self.blend = if blend.is_finite() { blend.clamp(0.0, 1.0) } else { DEFAULT_BLEND };
        self
    }

    /// Sets the easing of the item copy's travel from its slot to the card.
    ///
    /// The curve runs over the copy's own phase, from the start to the
    /// [`handoff`](Self::handoff): `0.0` is the copy on the item and `1.0` is the
    /// copy on the card. It only reshapes the travel; the fade between copy and
    /// page keeps following the timeline, and the copy still lands exactly on the
    /// card at the handoff, whatever the curve. The default is
    /// [`curves::LINEAR`](crate::curves::LINEAR), and it does nothing without an
    /// [`anchor`](Self::anchored).
    ///
    /// This composes with the easing the host already applies to the timeline,
    /// so pair it with a linear timeline when the element curves should be the
    /// only easing. An invalid curve (a non-finite Bézier control point) is
    /// ignored.
    pub fn anchor_curve(mut self, curve: impl Curve) -> Self {
        self.anchor_curve = valid_or(curve, self.anchor_curve);
        self
    }

    /// Sets the easing of the page's growth.
    ///
    /// The curve runs over the whole expansion and reshapes the page card's
    /// scale, position and corner radius. The page's fade, the dissolve and the
    /// dimming of the page behind keep following the timeline. The item copy
    /// follows the card, so it picks up this curve after the handoff. The
    /// default is [`curves::LINEAR`](crate::curves::LINEAR). A curve that overshoots makes the page
    /// overshoot too, but never gives it a negative corner radius. An invalid
    /// curve is ignored.
    ///
    /// Collapsing runs the same curve over reversed time, so the exit mirrors
    /// the entrance.
    pub fn page_curve(mut self, curve: impl Curve) -> Self {
        self.page_curve = valid_or(curve, self.page_curve);
        self
    }

    /// Samples the expansion at `progress`.
    ///
    /// Progress is clamped to `0.0..=1.0`; a non-finite value is treated as
    /// settled, the same as `1.0`.
    pub fn sample(&self, progress: f32) -> ContainerTransformFrame {
        let p = if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            1.0
        };
        // The page card follows its own curve; everything timed (fades,
        // dimming) follows the timeline itself.
        let q = self.page_curve.transform(p);
        let lerp = |from: f32, to: f32| from + (to - from) * q;

        let scale = lerp(self.source.width / self.viewport.width, 1.0);
        let (page_x, page_y) = (lerp(self.source.x, 0.0), lerp(self.source.y, 0.0));
        let transform = Mat3::translate(page_x, page_y).mul(&Mat3::scale(scale, scale));
        let backdrop = PaintPresentation {
            opacity: 1.0 - self.scrim * p,
            ..PaintPresentation::IDENTITY_OPAQUE
        };
        // A curve that overshoots must not carry the radius below zero: a
        // negative radius is declined by the host and the page would blink out.
        let corner_radius = lerp(self.source_radius, self.page_radius).max(0.0);

        if !self.anchored {
            return ContainerTransformFrame {
                page: PaintPresentation {
                    transform,
                    opacity: (p / FADE_IN_END).min(1.0),
                    corner_radius,
                },
                backdrop,
                anchor: PaintPresentation { opacity: 0.0, ..PaintPresentation::IDENTITY_OPAQUE },
            };
        }

        // How far through the dissolve: 0 before it, 1 after it.
        let half = self.blend / 2.0;
        let dissolve = if self.blend > 0.0 {
            ((p - (self.handoff - half)) / self.blend).clamp(0.0, 1.0)
        } else if p >= self.handoff {
            1.0
        } else {
            0.0
        };
        // How far the item has travelled to the card: it arrives at the handoff
        // and then keeps pace with the card.
        let lead = if self.handoff > 0.0 { self.anchor_curve.transform((p / self.handoff).min(1.0)) } else { 1.0 };
        // The item scaled to the card's width, which keeps the item's own
        // proportions, anchored at the card's corner.
        let card_scale = scale * self.viewport.width / self.source.width;
        let anchor_scale = 1.0 + (card_scale - 1.0) * lead;
        let anchor_x = self.source.x + (page_x - self.source.x) * lead;
        let anchor_y = self.source.y + (page_y - self.source.y) * lead;

        ContainerTransformFrame {
            page: PaintPresentation {
                transform,
                // Fully in by the middle of the dissolve: the item above it is
                // still half there, so nothing behind shows through.
                opacity: (dissolve * 2.0).min(1.0),
                corner_radius,
            },
            backdrop,
            anchor: PaintPresentation {
                transform: Mat3::translate(anchor_x, anchor_y).mul(&Mat3::scale(anchor_scale, anchor_scale)),
                opacity: 1.0 - dissolve,
                corner_radius: 0.0,
            },
        }
    }
}

/// `curve` unless it has a non-finite control point, in which case `fallback`.
fn valid_or(curve: impl Curve, fallback: AnyCurve) -> AnyCurve {
    if curve.is_valid() { curve.into_any() } else { fallback }
}

fn sanitize(radius: f32) -> f32 {
    if radius.is_finite() {
        radius.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::curve::curves;

    const VIEWPORT: ResolvedSize = ResolvedSize {
        width: 400.0,
        height: 800.0,
    };

    fn spec() -> ContainerTransformSpec {
        ContainerTransformSpec::new(ElementBounds::new(20.0, 120.0, 160.0, 160.0), VIEWPORT)
            .unwrap()
    }

    fn point(frame: &ContainerTransformFrame, x: f32, y: f32) -> (f32, f32) {
        frame.page.transform.transform_point(x, y)
    }

    #[test]
    fn at_the_start_the_page_sits_on_the_item_at_the_items_width() {
        let frame = spec().sample(0.0);

        assert_eq!(point(&frame, 0.0, 0.0), (20.0, 120.0));
        // The page's right edge lands on the item's right edge.
        assert_eq!(point(&frame, VIEWPORT.width, 0.0), (180.0, 120.0));
        // The page keeps its proportions, so it is taller than the item.
        assert_eq!(point(&frame, 0.0, VIEWPORT.height), (20.0, 120.0 + 320.0));
        assert_eq!(frame.page.corner_radius, 12.0);
        assert_eq!(frame.page.opacity, 0.0);
    }

    #[test]
    fn at_the_end_the_page_fills_the_viewport_unchanged() {
        let frame = spec().sample(1.0);

        assert_eq!(frame.page.transform, Mat3::identity());
        assert_eq!(frame.page.corner_radius, 0.0);
        assert_eq!(frame.page.opacity, 1.0);
        assert_eq!(frame.backdrop.opacity, 1.0 - 0.35);
    }

    #[test]
    fn halfway_is_halfway_in_scale_position_and_radius() {
        let frame = spec().sample(0.5);

        // Scale goes 0.4 -> 1.0, position (20, 120) -> (0, 0).
        assert_eq!(point(&frame, 0.0, 0.0), (10.0, 60.0));
        let right = point(&frame, VIEWPORT.width, 0.0);
        assert!((right.0 - (10.0 + 0.7 * 400.0)).abs() < 1e-3, "{right:?}");
        assert_eq!(right.1, 60.0);
        assert_eq!(frame.page.corner_radius, 6.0);
        assert_eq!(frame.page.opacity, 1.0);
    }

    #[test]
    fn the_page_grows_and_its_corners_straighten_monotonically() {
        let spec = spec().source_radius(24.0).page_radius(4.0);
        let mut last_width = f32::NEG_INFINITY;
        let mut last_radius = f32::INFINITY;

        for step in 0..=20 {
            let frame = spec.sample(step as f32 / 20.0);
            let width = point(&frame, VIEWPORT.width, 0.0).0 - point(&frame, 0.0, 0.0).0;
            assert!(width >= last_width, "step {step}");
            assert!(frame.page.corner_radius <= last_radius, "step {step}");
            last_width = width;
            last_radius = frame.page.corner_radius;
        }
        assert_eq!(spec.sample(1.0).page.corner_radius, 4.0);
    }

    #[test]
    fn the_page_fades_in_early_and_the_backdrop_dims_steadily() {
        let spec = spec().scrim(0.5);

        assert_eq!(spec.sample(0.0).page.opacity, 0.0);
        assert_eq!(spec.sample(0.125).page.opacity, 0.5);
        assert_eq!(spec.sample(0.25).page.opacity, 1.0);
        assert_eq!(spec.sample(0.0).backdrop.opacity, 1.0);
        assert_eq!(spec.sample(0.5).backdrop.opacity, 0.75);
        assert_eq!(spec.sample(1.0).backdrop.opacity, 0.5);
    }

    #[test]
    fn every_sampled_presentation_can_be_shown() {
        for step in 0..=50 {
            let frame = spec().sample(step as f32 / 50.0);
            assert!(frame.page.is_finite() && frame.backdrop.is_finite());
        }
    }

    #[test]
    fn out_of_range_progress_clamps_and_non_finite_progress_settles() {
        let spec = spec();

        assert_eq!(spec.sample(-3.0), spec.sample(0.0));
        assert_eq!(spec.sample(9.0), spec.sample(1.0));
        assert_eq!(spec.sample(f32::NAN), spec.sample(1.0));
        assert_eq!(spec.sample(f32::NEG_INFINITY), spec.sample(1.0));
    }

    #[test]
    fn invalid_geometry_is_rejected() {
        let good = ElementBounds::new(0.0, 0.0, 10.0, 10.0);
        for source in [
            ElementBounds::new(0.0, 0.0, 0.0, 10.0),
            ElementBounds::new(0.0, 0.0, 10.0, -1.0),
            ElementBounds::new(f32::NAN, 0.0, 10.0, 10.0),
            ElementBounds::new(0.0, 0.0, f32::INFINITY, 10.0),
        ] {
            assert!(ContainerTransformSpec::new(source, VIEWPORT).is_none());
        }
        for viewport in [
            ResolvedSize {
                width: 0.0,
                height: 800.0,
            },
            ResolvedSize {
                width: 400.0,
                height: f32::NAN,
            },
        ] {
            assert!(ContainerTransformSpec::new(good, viewport).is_none());
        }
    }

    #[test]
    fn radii_and_scrim_are_sanitized() {
        let spec = spec()
            .source_radius(-5.0)
            .page_radius(f32::NAN)
            .scrim(f32::NAN);

        let frame = spec.sample(0.0);
        assert_eq!(frame.page.corner_radius, 0.0);
        assert_eq!(spec.sample(1.0).page.corner_radius, 0.0);
        assert_eq!(spec.sample(1.0).backdrop.opacity, 1.0);
        assert_eq!(self::spec().scrim(7.0).sample(1.0).backdrop.opacity, 0.0);
    }

    /// The spec with an anchor (the item drawn as an overlay) and the default
    /// 25% handoff dissolved over 10% of the timeline.
    fn anchored() -> ContainerTransformSpec {
        spec().anchored(true)
    }

    fn corner_of(presentation: &PaintPresentation, x: f32, y: f32) -> (f32, f32) {
        presentation.transform.transform_point(x, y)
    }

    #[test]
    fn at_the_start_the_anchor_is_the_item_itself_and_the_page_is_hidden() {
        let frame = anchored().sample(0.0);

        // The item is 160x160 at (20, 120): the overlay copy lies exactly on it.
        assert_eq!(corner_of(&frame.anchor, 0.0, 0.0), (20.0, 120.0));
        assert_eq!(corner_of(&frame.anchor, 160.0, 160.0), (180.0, 280.0));
        assert_eq!(frame.anchor.opacity, 1.0);
        assert_eq!(frame.page.opacity, 0.0);
    }

    #[test]
    fn at_the_handoff_the_anchor_has_grown_to_the_pages_card() {
        let frame = anchored().sample(0.25);

        // The anchor's top-left and width are the card's: the page is scaled to
        // the same width and anchored at the same corner.
        let anchor_left = corner_of(&frame.anchor, 0.0, 0.0);
        let anchor_right = corner_of(&frame.anchor, 160.0, 0.0);
        let card_left = corner_of(&frame.page, 0.0, 0.0);
        let card_right = corner_of(&frame.page, VIEWPORT.width, 0.0);
        assert!((anchor_left.0 - card_left.0).abs() < 1e-3 && (anchor_left.1 - card_left.1).abs() < 1e-3);
        assert!((anchor_right.0 - card_right.0).abs() < 1e-3, "{anchor_right:?} vs {card_right:?}");
        // The anchor is a uniform scale, so its height follows its width.
        let scale = (anchor_right.0 - anchor_left.0) / 160.0;
        let anchor_bottom = corner_of(&frame.anchor, 0.0, 160.0);
        assert!((anchor_bottom.1 - anchor_left.1 - 160.0 * scale).abs() < 1e-3);
    }

    #[test]
    fn the_handoff_dissolves_the_anchor_into_the_page() {
        let spec = anchored();
        // Default: handoff at 0.25, dissolve over 0.1, so 0.20 to 0.30.
        let opacities = |p: f32| {
            let frame = spec.sample(p);
            (frame.anchor.opacity, frame.page.opacity)
        };

        assert_eq!(opacities(0.0), (1.0, 0.0));
        assert_eq!(opacities(0.19), (1.0, 0.0));
        let (anchor, page) = opacities(0.25);
        assert!((anchor - 0.5).abs() < 1e-3);
        // The page is fully in by the middle of the dissolve, so the two never
        // add up to less than the page and nothing shows through.
        assert!((page - 1.0).abs() < 1e-3, "{page}");
        let (anchor, page) = opacities(0.30);
        assert!(anchor.abs() < 1e-3 && (page - 1.0).abs() < 1e-3);
        assert_eq!(opacities(1.0), (0.0, 1.0));
    }

    #[test]
    fn after_the_handoff_the_anchor_stays_with_the_card() {
        let spec = anchored();

        for step in 25..=100 {
            let frame = spec.sample(step as f32 / 100.0);
            let anchor = corner_of(&frame.anchor, 0.0, 0.0);
            let card = corner_of(&frame.page, 0.0, 0.0);
            assert!((anchor.0 - card.0).abs() < 1e-3 && (anchor.1 - card.1).abs() < 1e-3, "step {step}");
        }
    }

    #[test]
    fn the_anchor_grows_monotonically_during_its_phase() {
        let spec = anchored();
        let mut last = 0.0;

        for step in 0..=25 {
            let frame = spec.sample(step as f32 / 100.0);
            let width = corner_of(&frame.anchor, 160.0, 0.0).0 - corner_of(&frame.anchor, 0.0, 0.0).0;
            assert!(width >= last, "step {step}");
            last = width;
        }
        assert!(last > 160.0);
    }

    #[test]
    fn the_handoff_and_dissolve_are_configurable_and_sanitized() {
        let late = anchored().handoff(0.5).blend(0.2);
        assert_eq!(late.sample(0.39).anchor.opacity, 1.0);
        assert!((late.sample(0.5).anchor.opacity - 0.5).abs() < 1e-3);
        assert_eq!(late.sample(0.61).anchor.opacity, 0.0);

        // A zero-width dissolve is a hard swap at the handoff.
        let hard = anchored().blend(0.0);
        assert_eq!(hard.sample(0.24).anchor.opacity, 1.0);
        assert_eq!(hard.sample(0.25).anchor.opacity, 0.0);
        assert_eq!(hard.sample(0.25).page.opacity, 1.0);

        // Garbage falls back to the defaults instead of poisoning the frame.
        let garbage = anchored().handoff(f32::NAN).blend(f32::INFINITY);
        assert_eq!(garbage.sample(0.0), anchored().sample(0.0));
        assert!(garbage.sample(0.4).anchor.is_finite());
        assert!(anchored().handoff(7.0).sample(0.5).anchor.is_finite());
    }

    #[test]
    fn without_an_anchor_the_page_fades_in_by_itself_as_before() {
        let frame = spec().sample(0.125);

        assert_eq!(frame.anchor.opacity, 0.0);
        assert_eq!(frame.page.opacity, 0.5);
    }

    #[test]
    fn every_anchored_presentation_can_be_shown() {
        for step in 0..=50 {
            let frame = anchored().sample(step as f32 / 50.0);
            assert!(frame.page.is_finite() && frame.backdrop.is_finite() && frame.anchor.is_finite());
        }
    }

    mod element_curves {
        use super::*;

        /// The overshooting curve used to check that nothing becomes invalid.
        const OVERSHOOT: curves::CubicBezier = curves::cubic_bezier(0.3, 1.8, 0.6, 1.0);

        fn all_curves() -> [AnyCurve; 5] {
            [
                curves::LINEAR.into_any(),
                curves::EASE_IN.into_any(),
                curves::EASE_OUT.into_any(),
                curves::BOUNCE_OUT.into_any(),
                OVERSHOOT.into_any(),
            ]
        }

        #[test]
        fn the_default_curves_change_nothing() {
            let plain = anchored();
            let explicit = anchored().anchor_curve(curves::LINEAR).page_curve(curves::LINEAR);

            for step in 0..=20 {
                let p = step as f32 / 20.0;
                assert_eq!(plain.sample(p), explicit.sample(p));
            }
        }

        #[test]
        fn the_page_curve_reshapes_the_pages_growth_but_not_its_fade() {
            let eased = spec().page_curve(curves::EASE_IN);
            let linear = spec();

            let at_half = eased.sample(0.5);

            // The page is where a linear page would be at EaseIn(0.5) = 0.125.
            let reference = linear.sample(curves::EASE_IN.transform(0.5));
            assert_eq!(at_half.page.transform, reference.page.transform);
            assert_eq!(at_half.page.corner_radius, reference.page.corner_radius);
            // Its fade-in and the dimming behind it keep following the timeline.
            assert_eq!(at_half.page.opacity, linear.sample(0.5).page.opacity);
            assert_eq!(at_half.backdrop, linear.sample(0.5).backdrop);
        }

        #[test]
        fn the_anchor_curve_reshapes_the_copys_travel() {
            let eased = anchored().anchor_curve(curves::EASE_IN);
            let at = 0.125; // halfway through the 0 to 25% phase

            let frame = eased.sample(at);

            // Halfway through its phase, EaseIn has covered 12.5% of the way to the card.
            let card = anchored().sample(at).page.transform.transform_point(0.0, 0.0);
            let expected = (20.0 + (card.0 - 20.0) * 0.125, 120.0 + (card.1 - 120.0) * 0.125);
            let got = frame.anchor.transform.transform_point(0.0, 0.0);
            assert!((got.0 - expected.0).abs() < 1e-3 && (got.1 - expected.1).abs() < 1e-3, "{got:?} vs {expected:?}");
            // The fade is untouched.
            assert_eq!(frame.anchor.opacity, anchored().sample(at).anchor.opacity);
        }

        #[test]
        fn the_copy_still_lands_on_the_card_at_the_handoff_with_any_curves() {
            for anchor in all_curves() {
                for page in all_curves() {
                    let frame = anchored().anchor_curve(anchor.clone()).page_curve(page.clone()).sample(0.25);

                    let copy = frame.anchor.transform.transform_point(0.0, 0.0);
                    let card = frame.page.transform.transform_point(0.0, 0.0);
                    assert!((copy.0 - card.0).abs() < 1e-2 && (copy.1 - card.1).abs() < 1e-2,
                        "{anchor:?}/{page:?}: {copy:?} vs {card:?}");
                    let copy_right = frame.anchor.transform.transform_point(160.0, 0.0).0;
                    let card_right = frame.page.transform.transform_point(VIEWPORT.width, 0.0).0;
                    assert!((copy_right - card_right).abs() < 1e-2, "{anchor:?}/{page:?}");
                }
            }
        }

        #[test]
        fn the_copy_keeps_pace_with_the_card_after_the_handoff_whatever_the_curves() {
            let spec = anchored().anchor_curve(curves::EASE_IN).page_curve(OVERSHOOT);

            for step in 25..=100 {
                let frame = spec.sample(step as f32 / 100.0);
                let copy = frame.anchor.transform.transform_point(0.0, 0.0);
                let card = frame.page.transform.transform_point(0.0, 0.0);
                assert!((copy.0 - card.0).abs() < 1e-2 && (copy.1 - card.1).abs() < 1e-2, "step {step}");
            }
        }

        #[test]
        fn overshooting_curves_never_make_a_presentation_invalid() {
            // An overshoot would push the corner radius negative, and a frame with
            // a negative radius is declined by the host: the page would blink out.
            for curve in all_curves() {
                let spec = anchored().anchor_curve(curve.clone()).page_curve(curve.clone()).source_radius(12.0).page_radius(0.0);
                for step in 0..=100 {
                    let frame = spec.sample(step as f32 / 100.0);
                    assert!(frame.page.is_finite() && frame.anchor.is_finite() && frame.backdrop.is_finite(),
                        "{curve:?} at step {step}");
                    assert!(frame.page.corner_radius >= 0.0, "{curve:?} at step {step}");
                }
            }
        }

        #[test]
        fn the_ends_are_unchanged_by_curves() {
            let spec = anchored().anchor_curve(curves::EASE_OUT).page_curve(curves::EASE_IN);

            assert_eq!(spec.sample(0.0).anchor.transform, anchored().sample(0.0).anchor.transform);
            assert_eq!(spec.sample(1.0).page.transform, Mat3::identity());
            assert_eq!(spec.sample(1.0).page.corner_radius, 0.0);
        }

        #[test]
        fn an_invalid_curve_is_ignored() {
            let bad = curves::cubic_bezier(f32::NAN, 0.0, 1.0, 1.0);
            let spec = anchored().anchor_curve(bad).page_curve(bad);

            for step in 0..=20 {
                let p = step as f32 / 20.0;
                assert_eq!(spec.sample(p), anchored().sample(p));
            }
        }

        #[test]
        fn the_anchor_curve_does_nothing_without_an_anchor() {
            let eased = spec().anchor_curve(curves::EASE_IN);

            for step in 0..=10 {
                let p = step as f32 / 10.0;
                assert_eq!(eased.sample(p), spec().sample(p));
            }
        }
    }
}
