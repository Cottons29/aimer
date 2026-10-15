//! Lucide icons embedded directly in Rust applications.
//!
//! Release builds parse and validate the collection at build time; debug builds
//! parse an icon on first use. Both cache the retained scene independently of
//! widget styling. Adding an SVG to `icons/` automatically adds its constant.
//!
//! ```
//! use aimer_lucide::Icons;
//! use aimer_svg::{Svg, SvgColor, SvgStyle};
//! let house = Svg::new(Icons::HOUSE).width(24).height(24)
//!     .style("path", SvgStyle::new().stroke(SvgColor::rgba8(0, 128, 255, 255)));
//! ```

/// The bundled Lucide collection, with uppercase snake-case icon constants.
///
/// For example, `arrow-down.svg` becomes [`Icons::ARROW_DOWN`]. Constants
/// reference shared descriptors; styling belongs to each [`aimer_svg::Svg`].
pub struct Icons;

#[cfg(aimer_lucide_precompiled)]
mod precompiled;

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

#[cfg(test)]
#[path = "../build_support.rs"]
#[allow(dead_code)]
mod generation;
