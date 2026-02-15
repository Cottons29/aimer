use crate::{Widget, base::*};
use std::any::Any;

pub trait StatefulWidget: Send + Sync {
    fn draw(&self, ctx: &BuildContext);
    fn set_state(&mut self, function: impl Fn(&mut Self)) {
        function(self);
    }
}

