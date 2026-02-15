use crate::{StatefulWidget, base::*};
use std::any::Any;
use crate::Widget;
pub trait StatelessWidget: Send + Sync {
    fn draw(&self, ctx: &BuildContext);
    fn pos(&self) -> Option<Vec2d> {
        None
    }
    fn size(&self) -> Option<Size> {
        None
    }
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        if self.size().is_none() || self.pos().is_none() {
            return None;
        }
        let start = self.pos().unwrap();
        let end = start.get_end(self.size().unwrap());
        Some((start, end))
    }
    fn on_click(&self) -> Option<&Box<dyn Fn() + Send + Sync>> {
        None
    }
}

impl<T: StatelessWidget + 'static> From<T> for Box<dyn StatelessWidget> {
    fn from(value: T) -> Self {
        Box::new(value)
    }
}



