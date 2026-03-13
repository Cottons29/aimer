
pub trait Route: Clone + Send + Sync + 'static {
    fn parse(path: &str) -> Option<Self> where Self: Sized;
    fn format(&self) -> String;
}
