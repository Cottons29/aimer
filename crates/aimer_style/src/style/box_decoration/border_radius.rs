use aimer_attribute::Dimension;
use crate::style::border::resolve_dim;
use aimer_macro::Constructor;

#[derive(Default, Clone, Copy, PartialEq, Debug, Constructor)]
pub struct BorderRadius {
    pub top_left: Dimension,
    pub top_right: Dimension,
    pub bottom_right: Dimension,
    pub bottom_left: Dimension,
}

impl BorderRadius {
    pub fn resolve(&self, box_width: f32, box_height: f32, scale: f32) -> [f32; 4] {
        [
            resolve_dim(self.top_left, box_width, scale),
            resolve_dim(self.top_right, box_width, scale),
            resolve_dim(self.bottom_right, box_height, scale),
            resolve_dim(self.bottom_left, box_height, scale),
        ]
    }
}

impl From<i32> for BorderRadius {
    fn from(value: i32) -> Self {
        BorderRadius { top_left: value.into(), top_right: value.into(), bottom_right: value.into(), bottom_left: value.into() }
    }
}

impl From<f32> for BorderRadius {
    fn from(value: f32) -> Self {
        BorderRadius { top_left: value.into(), top_right: value.into(), bottom_right: value.into(), bottom_left: value.into() }
    }
}

impl From<(f32, f32, f32, f32)> for BorderRadius {
    fn from((tl, tr, br, bl): (f32, f32, f32, f32)) -> Self {
        BorderRadius { top_left: tl.into(), top_right: tr.into(), bottom_right: br.into(), bottom_left: bl.into() }
    }
}

impl From<(i32, i32, i32, i32)> for BorderRadius {
    fn from((tl, tr, br, bl): (i32, i32, i32, i32)) -> Self {
        BorderRadius { top_left: tl.into(), top_right: tr.into(), bottom_right: br.into(), bottom_left: bl.into() }
    }
}
