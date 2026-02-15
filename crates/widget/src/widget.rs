pub mod stateful;
pub mod stateless;
use crate::{StatefulWidget, base::*};
use crate::{StatelessWidget, base::BuildContext};
#[allow(dead_code)]
pub trait Widget: Send + Sync {
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
    fn on_click(&self) -> Option<&(dyn Fn() + Send + Sync)> {
        None
    }
    fn child(&self) -> &[Box<dyn Widget>] {
        &[]
    }
}

impl<T: Widget + 'static> From<T> for Box<dyn Widget> {
    fn from(value: T) -> Self {
        Box::new(value) 
    }
}
struct _Stateless<T: StatelessWidget>(T);
struct _Stateful<T: StatefulWidget>(T);

impl<T: StatelessWidget> Widget for _Stateless<T> {
    fn draw(&self, ctx: &BuildContext) {
        self.0.draw(ctx);
    }
}

impl<T: StatefulWidget> Widget for _Stateful<T> {
    fn draw(&self, ctx: &BuildContext) {
        self.0.draw(ctx);
    }
}
