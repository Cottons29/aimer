use std::sync::Arc;

use super::{SvgColor, SvgTransform};

/// Coordinate space used by a gradient.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SvgGradientUnits {
    /// Coordinates are relative to the painted object's bounding box.
    ObjectBoundingBox,
    /// Coordinates are expressed in the current user space.
    UserSpaceOnUse,
}

/// Repeat behavior after a gradient reaches its final stop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SvgSpreadMethod {
    /// Clamp the edge colors outside the gradient interval.
    Pad,
    /// Reflect the gradient on each repetition.
    Reflect,
    /// Repeat the gradient from its first stop.
    Repeat,
}

/// A finite gradient stop retained from an SVG document.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgGradientStop {
    /// Stop position in the normalized range 0..=1.
    pub offset: f32,
    /// Stop color, including `stop-opacity`.
    pub color: SvgColor,
}

impl SvgGradientStop {
    /// Returns whether the stop can be safely passed to a renderer.
    pub fn is_finite(self) -> bool {
        self.offset.is_finite() && self.color.is_finite()
    }
}

/// A parsed linear or radial SVG gradient.
#[derive(Clone, Debug, PartialEq)]
pub enum SvgGradient {
    /// A linear gradient.
    Linear {
        /// Source id used by `url(#id)` paints.
        id: Arc<str>,
        /// Start x coordinate.
        x1: f32,
        /// Start y coordinate.
        y1: f32,
        /// End x coordinate.
        x2: f32,
        /// End y coordinate.
        y2: f32,
        /// Coordinate interpretation.
        units: SvgGradientUnits,
        /// Gradient-local transform.
        transform: SvgTransform,
        /// Out-of-range spread behavior.
        spread: SvgSpreadMethod,
        /// Ordered normalized stops.
        stops: Arc<[SvgGradientStop]>,
    },
    /// A radial gradient.
    Radial {
        /// Source id used by `url(#id)` paints.
        id: Arc<str>,
        /// Center x coordinate.
        cx: f32,
        /// Center y coordinate.
        cy: f32,
        /// Outer radius.
        radius: f32,
        /// Focal x coordinate.
        fx: f32,
        /// Focal y coordinate.
        fy: f32,
        /// Focal radius.
        focal_radius: f32,
        /// Coordinate interpretation.
        units: SvgGradientUnits,
        /// Gradient-local transform.
        transform: SvgTransform,
        /// Out-of-range spread behavior.
        spread: SvgSpreadMethod,
        /// Ordered normalized stops.
        stops: Arc<[SvgGradientStop]>,
    },
}

impl SvgGradient {
    /// Returns the source id used to reference this gradient.
    pub fn id(&self) -> &str {
        match self {
            Self::Linear { id, .. } | Self::Radial { id, .. } => id,
        }
    }

    /// Returns the retained stops in source order.
    pub fn stops(&self) -> &[SvgGradientStop] {
        match self {
            Self::Linear { stops, .. } | Self::Radial { stops, .. } => stops,
        }
    }

    /// Returns whether every numeric value is finite.
    pub fn is_finite(&self) -> bool {
        match self {
            Self::Linear {
                x1,
                y1,
                x2,
                y2,
                transform,
                stops,
                ..
            } => [*x1, *y1, *x2, *y2].into_iter().all(f32::is_finite)
                && transform.is_finite()
                && stops.iter().all(|stop| stop.is_finite()),
            Self::Radial {
                cx,
                cy,
                radius,
                fx,
                fy,
                focal_radius,
                transform,
                stops,
                ..
            } => [*cx, *cy, *radius, *fx, *fy, *focal_radius]
                .into_iter()
                .all(f32::is_finite)
                && transform.is_finite()
                && stops.iter().all(|stop| stop.is_finite()),
        }
    }
}

/// A paint retained by the parser, including paints the current GPU path has
/// not yet submitted.
#[derive(Clone, Debug, PartialEq)]
pub enum SvgPaint {
    /// A solid color.
    Solid(SvgColor),
    /// A linear gradient.
    Linear(SvgGradient),
    /// A radial gradient.
    Radial(SvgGradient),
    /// A pattern reference whose tile rendering is deferred.
    Pattern { id: Arc<str> },
}

impl SvgPaint {
    /// Returns whether all retained numeric values are finite.
    pub fn is_finite(&self) -> bool {
        match self {
            Self::Solid(color) => color.is_finite(),
            Self::Linear(gradient) | Self::Radial(gradient) => gradient.is_finite(),
            Self::Pattern { .. } => true,
        }
    }
}
