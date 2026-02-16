use crate::{Widget, base::*};
use std::any::Any;

pub trait StatefulWidget: Send + Sync {
    type State;
    fn draw(&self, ctx: &BuildContext);
    fn set_state(&mut self, function: impl Fn(&mut Self)) {
        function(self);
    }
    fn create_state(&self) -> Self::State;
}

