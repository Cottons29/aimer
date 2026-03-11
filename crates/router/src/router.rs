
pub trait Router {
    fn path(&self) -> String;
    fn from_path(path: &str) -> Self where Self: Sized;
}

pub trait RouteParser<R> {
    fn parse(path: &str) -> R;
    fn format(route: &R) -> String;
}