#![allow(clippy::main_recursion)]

pub mod api;
mod blog_store;
mod components;
mod router;
mod screen;
mod utils;

use std::sync::atomic::{AtomicBool, AtomicUsize};

use aimer::router::Navigator;
use aimer::*;

use crate::router::AppRouter;
pub static TEST_STATE_UPDATED: AtomicBool = AtomicBool::new(false);
pub static CURRENT_INDEX: AtomicUsize = AtomicUsize::new(0);

// this is the entry point of the app
#[aimer::main]
fn main() {
    AimerApp::new()
        .child(Navigator::<AppRouter>::new(AppRouter::Home, |route| {
            route.boxed()
        }))
        .run();
}