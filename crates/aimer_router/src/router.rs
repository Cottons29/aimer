use aimer_widget::base::BuildContext;
use aimer_widget::Widget;

pub trait Route: Clone + Send + Sync + 'static {
    fn parse(path: &str) -> Option<Self> where Self: Sized;
    fn format(&self) -> String;
}

pub trait Router {
    fn build(&self, ctx: &BuildContext) -> Box<dyn Widget>;
}
