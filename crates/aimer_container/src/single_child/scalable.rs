use aimer_widget::base::BuildContext;
use aimer_widget::{AnyElement, RequiredChild, Widget};
#[allow(dead_code)]
pub struct Scalable<W = RequiredChild> {
    scale: f32,
    child: W,
}

#[allow(dead_code)]
impl Scalable {
    #[inline]
    pub fn new() -> Self {
        Self {
            child: RequiredChild,
            scale: 1.0,
        }
    }

    #[inline]
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    #[inline]
    pub fn child<W: Widget>(self, child: W) -> Scalable<W> {
        Scalable {
            child,
            scale: self.scale,
        }
    }
}

impl<W: Widget + 'static> Widget for Scalable<W> {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        todo!()
    }
}
