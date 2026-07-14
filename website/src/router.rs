use std::time::Duration;

use aimer::animation::{AnimatedSwitcher, Curve};
use aimer::router::{Router, Shell};
use aimer::style::{TextAlign, TextStyle};
use aimer::*;

use crate::components::app_shell::AppShell;
use crate::screen::docs_screen::DocsPage;
use crate::screen::home_screen::HomePage;
use crate::screen::learn_screen::LearnPage;

const ROUTE_TRANSITION_DURATION: Duration = Duration::from_millis(200);
const ROUTE_SWITCHER_KEY: &str = "route-switcher";

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

    fn transition_key(&self) -> &'static str {
        match self {
            AppRouter::Home => "home",
            AppRouter::Docs => "docs",
            AppRouter::Learn => "learn",
            AppRouter::NotFound => "not-found",
        }
    }
}

fn transitioned_page(
    key: &'static str,
    child: impl Widget + 'static,
) -> AnimatedSwitcher<Box<dyn Widget>> {
    AnimatedSwitcher::new(ROUTE_TRANSITION_DURATION, Curve::FastOutSlowIn, child.boxed())
        .child_key(key)
        .key(ROUTE_SWITCHER_KEY)
}

impl Router for AppRouter {
    fn build(&self, _ctx: &BuildContext) -> Box<dyn Widget> {
        // Every route renders inside the same persistent app shell (header +
        // content area). Only the shell's `Outlet` child — the page below —
        // changes as we navigate.
        let active_tab = self.active_tab();
        let transition_key = self.transition_key();
        match self {
            AppRouter::Home => Shell::boxing(AppShell { active_tab }, move |ctx| {
                transitioned_page(transition_key, HomePage::boxing(ctx)).boxed()
            }),
            AppRouter::Docs => Shell::boxing(AppShell { active_tab }, move |ctx| {
                transitioned_page(transition_key, DocsPage::boxing(ctx)).boxed()
            }),
            AppRouter::Learn => Shell::boxing(AppShell { active_tab }, move |ctx| {
                transitioned_page(transition_key, LearnPage::boxing(ctx)).boxed()
            }),
            AppRouter::NotFound => Shell::boxing(AppShell { active_tab }, move |_| {
                transitioned_page(transition_key, not_found_page()).boxed()
            }),
        }
    }
}

/// A simple "page not found" placeholder rendered inside the shell content area.
fn not_found_page() -> impl Widget {
    Container::new().color(Color::WHITE).child(
        Column::new()
            .horizontal_alignment(BoxAlignment::Center)
            .vertical_alignment(BoxAlignment::Center)
            .children(vec![
                Text::new("Page not found")
                    .text_align(TextAlign::MidCenter)
                    .text_style(TextStyle::new().font_size(44)),
            ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_have_stable_distinct_transition_keys() {
        let keys = [
            AppRouter::Home.transition_key(),
            AppRouter::Docs.transition_key(),
            AppRouter::Learn.transition_key(),
            AppRouter::NotFound.transition_key(),
        ];

        assert_eq!(keys, ["home", "docs", "learn", "not-found"]);
    }

    #[test]
    fn route_transition_uses_the_expected_duration() {
        assert_eq!(ROUTE_TRANSITION_DURATION, Duration::from_millis(200));
    }

    #[test]
    fn route_transition_has_stable_switcher_identity() {
        assert_eq!(
            Widget::key(&transitioned_page("home", not_found_page())),
            Some(Key::Value(ROUTE_SWITCHER_KEY.to_owned()))
        );
    }

    #[test]
    fn route_transition_erases_page_type_for_state_reuse() {
        let _: AnimatedSwitcher<Box<dyn Widget>> = transitioned_page("home", not_found_page());
    }
}
