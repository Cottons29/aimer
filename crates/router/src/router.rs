use widget::base::BuildContext;
use widget::Widget;

pub trait Route: Clone + Send + Sync + 'static {
    fn parse(path: &str) -> Option<Self> where Self: Sized;
    fn format(&self) -> String;
}

pub trait Router: Widget {
    fn build(&self, ctx: &BuildContext) -> Box<dyn Widget>;
}
