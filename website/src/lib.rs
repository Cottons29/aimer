mod components;
mod router;
mod screen;
mod utils;

use crate::router::AppRouter;
use aimer::router::Navigator;
use aimer::*;

// this is the entry point of the app
#[main]
pub fn my_app() {
    AimerApp::start(Navigator::<AppRouter>::new(AppRouter::Home, |route| Box::new(route)));
}

#[allow(unused)]
fn main() {
    AimerApp::start(Navigator::<AppRouter>::new(AppRouter::Home, |route| Box::new(route)));
}
