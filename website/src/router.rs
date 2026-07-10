use aimer::*;
use aimer::router::{Router, Shell};
use aimer::style::{TextAlign, TextStyle};

use crate::components::app_shell::AppShell;
use crate::screen::docs_screen::DocsPage;
use crate::screen::home_screen::HomePage;
use crate::screen::learn_screen::LearnPage;

#[widget(Router)]
#[derive(Clone)]
pub enum AppRouter {
    Home,
    Docs,
    Learn,
    NotFound,
}

impl AppRouter {
    /// The header tab index this route highlights (0 = Home, 1 = Docs, 2 = Learn).
    fn active_tab(&self) -> usize {
        match self {
            AppRouter::Docs => 1,
            AppRouter::Learn => 2,
            _ => 0,
        }
    }
}

impl Router for AppRouter {
    fn build(&self, _ctx: &BuildContext) -> Box<dyn Widget> {
        // Every route renders inside the same persistent app shell (header +
        // content area). Only the shell's `Outlet` child — the page below —
        // changes as we navigate.
        let active_tab = self.active_tab();
        match self {
            AppRouter::Home => Shell::new(AppShell { active_tab }, |_| Box::new(HomePage {})).boxed(),
            AppRouter::Docs => Shell::new(AppShell { active_tab }, |_| Box::new(DocsPage {})).boxed(),
            AppRouter::Learn => Shell::new(AppShell { active_tab }, |_| Box::new(LearnPage {})).boxed(),
            AppRouter::NotFound => Shell::new(AppShell { active_tab }, |_| Box::new(not_found_page())).boxed(),
        }
    }
}

/// A simple "page not found" placeholder rendered inside the shell content area.
fn not_found_page() -> impl Widget {
    Container!(
        color: Color::WHITE,
        child: Column!(
            horizontal_alignment: BoxAlignment::Center,
            vertical_alignment: BoxAlignment::Center,
            children: [
                Text!(
                    "Page not found",
                    text_align: TextAlign::MidCenter,
                    text_style: TextStyle!(
                        font_size: 44,
                    )
                )
            ]
        )
    )
}
