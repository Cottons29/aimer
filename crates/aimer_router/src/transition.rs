//! Page transitions selected once per navigation and sampled without rebuilding pages.
//!
//! A Navigator has no fallback effect until configured with `.transition(...)`.
//! Manual routes override [`Route::navigation_transition`] to return
//! [`TransitionChoice::custom`], [`TransitionChoice::Disabled`], or
//! [`TransitionChoice::Inherit`]. Generated routes support the same policy:
//!
//! ```text
//! #[route("/profile/{name}")]
//! #[transition(ZoomTransition::new())]
//! #[view_level(2)]
//! Profile { name: String },
//! ```
//!
//! `#[transition(none)]` disables a route's animation. Unspecified transitions
//! inherit the host fallback, and unspecified view levels are zero. Push,
//! replacement, clear, and branch changes select the destination's policy;
//! pop selects the departing page's policy. Custom effects receive the real
//! source, destination, and action and are responsible for any reversal.
//!
//! Shell frames stay stationary: use `Shell::new(frame, builder).route(child)`
//! and configure its fallback separately. `StatefulShell::transition` handles
//! visible branch changes. Routes generated with `#[shell]` automatically
//! declare the persistent boundary; manual routes override [`Route::shell_key`].
//!
//! The first page and zero-duration effects appear immediately. Navigating
//! during an animation snaps it to completion and starts the latest change,
//! retaining at most two pages. Only the incoming page receives input and
//! focus. Portable guests keep immediate swaps.
//!
//! # Custom effects
//!
//! ```
//! use std::time::Duration;
//! use aimer_router::{Curve, NavigationContext, NavigationTransition,
//!     NavigationTransitionFrame, PaintPresentation, ResolvedNavigationTransition, Route};
//!
//! struct FadeIn;
//! impl<R: Route> NavigationTransition<R> for FadeIn {
//!     fn resolve(&self, _: &NavigationContext<'_, '_, R>) -> Option<ResolvedNavigationTransition> {
//!         ResolvedNavigationTransition::new(Duration::from_millis(200), Curve::EaseOut, |p, _| {
//!             NavigationTransitionFrame {
//!                 outgoing: PaintPresentation::IDENTITY_OPAQUE,
//!                 incoming: PaintPresentation { opacity: p, ..PaintPresentation::IDENTITY_OPAQUE },
//!             }
//!         })
//!     }
//! }
//! ```

use std::rc::Rc;
use std::time::Duration;

pub use aimer_animation::{Curve, PaintPresentation};
use aimer_canvas::Mat3;
use aimer_widget::base::{BuildContext, ResolvedSize};

use crate::Route;

/// The operation that changed the visible route.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavigationAction {
    /// Append a route to a stack.
    #[default]
    Push,
    /// Return to the previous route, reversing the departing page's effect.
    Pop,
    /// Replace the stack with a route.
    Replace,
    /// Return to the initial route.
    Clear,
    /// Switch the active stateful-shell branch.
    BranchSwitch,
    /// Browser history navigation whose direction is unknown.
    History,
}

/// Effective routes and context supplied to a transition selector.
pub struct NavigationContext<'a, 'frame, R: Route> {
    /// The page being left, after redirects.
    pub from: &'a R,
    /// The destination, after redirects.
    pub to: &'a R,
    /// The operation that requested navigation.
    pub action: NavigationAction,
    /// The host's inherited state, available while resolving the effect.
    pub build_context: &'a BuildContext<'frame>,
}

/// Resolves an effect once per visible navigation.
///
/// Implementations may inspect routes and inherited state. Return `None` for
/// an immediate swap. The resolved sampler runs on the rendering thread;
/// it must be finite, allocation-free, and must not build widgets.
pub trait NavigationTransition<R: Route>: 'static {
    /// Selects an effect for these effective routes and this action.
    fn resolve(&self, navigation: &NavigationContext<'_, '_, R>) -> Option<ResolvedNavigationTransition>;
}

/// Per-route policy. An absent fallback also means an immediate swap.
pub enum TransitionChoice<R: Route> {
    /// Use the host's configured fallback.
    Inherit,
    /// Disable animation, overriding the host fallback.
    Disabled,
    /// Select an application or built-in effect.
    Custom(Rc<dyn NavigationTransition<R>>),
}

impl<R: Route> TransitionChoice<R> {
    /// Stores a transition as the route's explicit selection.
    #[inline]
    pub fn custom(transition: impl NavigationTransition<R>) -> Self {
        Self::Custom(Rc::new(transition))
    }
}

/// Presentation of the two retained pages at one instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavigationTransitionFrame {
    /// Presentation of the departing page, painted behind the incoming page.
    pub outgoing: PaintPresentation,
    /// Presentation of the arriving page.
    pub incoming: PaintPresentation,
}

impl NavigationTransitionFrame {
    pub(crate) const FINISHED: Self = Self {
        outgoing: PaintPresentation { transform: Mat3::identity(), opacity: 0.0 },
        incoming: PaintPresentation::IDENTITY_OPAQUE,
    };
}

/// Timing and a viewport-dependent sampler for one navigation.
pub struct ResolvedNavigationTransition {
    pub(crate) duration: Duration,
    pub(crate) curve: Curve,
    sampler: Box<dyn Fn(f32, ResolvedSize) -> NavigationTransitionFrame>,
}

impl ResolvedNavigationTransition {
    /// Creates a custom effect. Invalid easing control points return `None`.
    /// A zero duration is valid and tells the host to swap immediately.
    pub fn new(
        duration: Duration,
        curve: Curve,
        sampler: impl Fn(f32, ResolvedSize) -> NavigationTransitionFrame + 'static,
    ) -> Option<Self> {
        if let Curve::CubicBezier(a, b, c, d) = curve
            && ![a, b, c, d].iter().all(|v| v.is_finite()) {
            return None;
        }
        Some(Self { duration, curve, sampler: Box::new(sampler) })
    }

    /// Returns the configured playback duration.
    pub fn duration(&self) -> Duration { self.duration }

    /// Returns the configured easing curve.
    pub fn curve(&self) -> Curve { self.curve }

    /// Samples already-eased progress, clamped to `0..=1`, for a viewport in
    /// logical pixels (physical layout dimensions divided by display scale).
    /// Returns `None` for non-finite input, invalid viewport sizes, or invalid
    /// sampled presentation. Opacity is clamped to `0..=1`.
    pub fn sample(&self, progress: f32, size: ResolvedSize) -> Option<NavigationTransitionFrame> {
        if !progress.is_finite() || !size.width.is_finite() || !size.height.is_finite()
            || size.width < 0.0 || size.height < 0.0 { return None; }
        let mut frame = (self.sampler)(progress.clamp(0.0, 1.0), size);
        for presentation in [&mut frame.outgoing, &mut frame.incoming] {
            if !presentation.opacity.is_finite()
                || !presentation.transform.cols.iter().flatten().all(|v| v.is_finite()) {
                return None;
            }
            presentation.opacity = presentation.opacity.clamp(0.0, 1.0);
        }
        Some(frame)
    }
}

/// Cross-fades the departing and arriving pages over 300 ms by default.
#[derive(Clone, Copy, Debug)]
pub struct CrossFadeTransition {
    duration: Duration,
    curve: Curve,
}

impl Default for CrossFadeTransition {
    fn default() -> Self { Self::new() }
}

impl CrossFadeTransition {
    /// Creates a 300 ms transition with [`Curve::EaseOut`].
    #[inline]
    pub fn new() -> Self { Self { duration: Duration::from_millis(300), curve: Curve::EaseOut } }
    /// Sets the duration. Zero disables animation for this navigation.
    #[inline]
    pub fn duration(mut self, duration: Duration) -> Self { self.duration = duration; self }
    /// Sets easing. Non-finite Bézier control points disable this effect.
    #[inline]
    pub fn curve(mut self, curve: Curve) -> Self { self.curve = curve; self }
}

impl<R: Route> NavigationTransition<R> for CrossFadeTransition {
    fn resolve(&self, _: &NavigationContext<'_, '_, R>) -> Option<ResolvedNavigationTransition> {
        ResolvedNavigationTransition::new(self.duration, self.curve, |p, _| NavigationTransitionFrame {
            outgoing: PaintPresentation { transform: Mat3::identity(), opacity: 1.0 - p },
            incoming: PaintPresentation { transform: Mat3::identity(), opacity: p },
        })
    }
}

/// Scales pages around the viewport center while cross-fading them.
/// Pops play the departing page's effect backwards.
#[derive(Clone, Copy, Debug)]
pub struct ZoomTransition(CrossFadeTransition);

/// Slides between explicit route levels and cross-fades at equal levels.
/// Increasing levels enter from the right; decreasing levels enter from the
/// left. This direction is independent of the stack operation.
#[derive(Clone, Copy, Debug)]
pub struct ViewLevelTransitions(CrossFadeTransition);

macro_rules! timed_transition {
    ($name:ident) => {
        impl Default for $name { fn default() -> Self { Self::new() } }
        impl $name {
            /// Creates a 300 ms transition with [`Curve::EaseOut`].
            #[inline]
            pub fn new() -> Self { Self(CrossFadeTransition::new()) }
            /// Sets the duration. Zero performs an immediate swap.
            #[inline]
            pub fn duration(mut self, duration: Duration) -> Self { self.0.duration = duration; self }
            /// Sets easing. Non-finite Bézier control points disable the effect.
            #[inline]
            pub fn curve(mut self, curve: Curve) -> Self { self.0.curve = curve; self }
        }
    };
}

timed_transition!(ZoomTransition);
timed_transition!(ViewLevelTransitions);

impl<R: Route> NavigationTransition<R> for ZoomTransition {
    fn resolve(&self, navigation: &NavigationContext<'_, '_, R>) -> Option<ResolvedNavigationTransition> {
        let reverse = navigation.action == NavigationAction::Pop;
        ResolvedNavigationTransition::new(self.0.duration, self.0.curve, move |p, size| {
            let p = if reverse { 1.0 - p } else { p };
            let centered = |scale| Mat3::translate(size.width * 0.5, size.height * 0.5)
                .mul(&Mat3::scale(scale, scale))
                .mul(&Mat3::translate(-size.width * 0.5, -size.height * 0.5));
            let outgoing = PaintPresentation { transform: centered(1.0 + 0.08 * p), opacity: 1.0 - p };
            let incoming = PaintPresentation { transform: centered(0.92 + 0.08 * p), opacity: p };
            if reverse { NavigationTransitionFrame { outgoing: incoming, incoming: outgoing } }
            else { NavigationTransitionFrame { outgoing, incoming } }
        })
    }
}

impl<R: Route> NavigationTransition<R> for ViewLevelTransitions {
    fn resolve(&self, navigation: &NavigationContext<'_, '_, R>) -> Option<ResolvedNavigationTransition> {
        use std::cmp::Ordering;
        let sign = match navigation.to.view_level().cmp(&navigation.from.view_level()) {
            Ordering::Equal => return self.0.resolve(navigation),
            Ordering::Greater => 1.0,
            Ordering::Less => -1.0,
        };
        ResolvedNavigationTransition::new(self.0.duration, self.0.curve, move |p, size| {
            NavigationTransitionFrame {
                outgoing: PaintPresentation { transform: Mat3::translate(-sign * p * size.width, 0.0), opacity: 1.0 },
                incoming: PaintPresentation { transform: Mat3::translate(sign * (1.0 - p) * size.width, 0.0), opacity: 1.0 },
            }
        })
    }
}

/// Selects an effect at runtime from the routes, action, and inherited state.
/// The callback runs once per visible navigation, never per animation tick.
pub struct DynamicTransitions<F> { selector: F }

impl<F> DynamicTransitions<F> {
    /// Creates a runtime selector. Return `None` to swap immediately.
    #[inline]
    pub fn new(selector: F) -> Self { Self { selector } }
}

impl<R, F> NavigationTransition<R> for DynamicTransitions<F>
where
    R: Route,
    F: Fn(&NavigationContext<'_, '_, R>) -> Option<ResolvedNavigationTransition> + 'static,
{
    fn resolve(&self, navigation: &NavigationContext<'_, '_, R>) -> Option<ResolvedNavigationTransition> {
        (self.selector)(navigation)
    }
}
