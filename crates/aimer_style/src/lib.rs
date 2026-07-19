mod animated_theme;
mod style;
mod theme;

pub use animated_theme::AnimatedTheme;
pub use theme::{Theme, ThemeData};

// layout export
pub use aimer_attribute::BoxConstraint;
pub use style::alignment::{ColumnAlignment, RowAlignment};
// box decoration export
pub use style::border::Stroke;
pub use style::border::{BorderSlice, BorderStyle, BoxBorder, BoxOutline};
pub use style::box_decoration::BoxDecoration;
pub use style::box_decoration::border_radius::BorderRadius;
pub use style::box_decoration::box_shadow::{BoxShadow, ShadowSide};
pub use style::box_fit::BoxFit;
pub use style::layout_spacing::{LayoutSpacing, Spacing};
// text export
pub use style::text_style::*;
