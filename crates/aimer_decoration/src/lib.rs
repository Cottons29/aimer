//! Decorations and a layout-transparent widget for painting behind a child.

mod border;
mod box_decoration;
mod decoration;
mod decoration_box;
mod multi_decoration_box;
mod portable;

pub use border::{BorderSlice, BorderStyle, BoxBorder, BoxOutline, Stroke};
pub use box_decoration::BoxDecoration;
pub use box_decoration::border_radius::BorderRadius;
pub use box_decoration::box_shadow::{BoxShadow, ShadowSide};

pub use decoration_box::{
    BackdropFilter, DecorationBox, LightEdge, Magnification, MagnificationStyle,
};

pub use decoration::{AnyDecor, Decoration};
pub use multi_decoration_box::MultiDecorationBox;
