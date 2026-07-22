use aimer::animation::{AnimatedSwitcher, Curve};
use aimer::router::{Router, Shell};
use aimer::style::{TextAlign, TextStyle, Theme, ThemeData};
use aimer::*;
use std::panic::Location;
use std::time::Duration;

use crate::components::app_shell::AppShell;
use crate::screen::blog::BlogListPage;
use crate::screen::blog_detail::BlogDetailPage;
use crate::screen::home_screen::HomePage;
use crate::screen::learn_screen::LearnPage;

const ROUTE_TRANSITION_DURATION: Duration = Duration::from_millis(200);
const ROUTE_SWITCHER_KEY: &str = "route-switcher";

#[cfg(test)]
thread_local! {
    static ROUTE_BUILDS: std::cell::RefCell<Vec<AppRouter>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(crate) fn take_route_builds() -> Vec<AppRouter> {
    ROUTE_BUILDS.with(|builds| std::mem::take(&mut *builds.borrow_mut()))
}

#[widget(Router)]
#[derive(Clone, Debug, PartialEq)]
pub enum AppRouter {
    #[route("/")]
    Home,
    #[route("/blog")]
    Blog,
    #[route("/blog/{id}")]
    BlogDetail { id: String },
    #[route("/learn")]
    Learn,
    #[route("/not-found")]
    NotFound,
}

impl AppRouter {
    /// The header tab index this route highlights (0 = Home, 1 = Docs, 2 =
    /// Learn).
    fn active_tab(&self) -> usize {
        match self {
            AppRouter::Blog | AppRouter::BlogDetail { .. } => 1,
            AppRouter::Learn => 2,
            _ => 0,
        }
    }

    fn transition_key(&self) -> &'static str {
        match self {
            AppRouter::Home => "home",
            AppRouter::Blog => "blog",
            AppRouter::BlogDetail { .. } => "blog-detail",
            AppRouter::Learn => "learn",
            AppRouter::NotFound => "not-found",
        }
    }
}

fn transitioned_page(
    key: &'static str,
    child: impl Widget + 'static,
) -> AnimatedSwitcher<Box<dyn Widget>> {
    AnimatedSwitcher::new(
        ROUTE_TRANSITION_DURATION,
        Curve::FastOutSlowIn,
        child.boxed(),
    )
    .child_key(key)
    .key(ROUTE_SWITCHER_KEY)
}

impl Router for AppRouter {
    #[track_caller]
    fn build(&self, _ctx: &BuildContext) -> AnyWidget {
        // Every route renders inside the same persistent app shell (header +
        // content area). Only the shell's `Outlet` child — the page below —
        // changes as we navigate.

        #[cfg(test)]
        ROUTE_BUILDS.with(|builds| {
            builds
                .borrow_mut()
                .push(self.clone())
        });
        eprintln!("Current route: {:?}", self);
        let active_tab = self.active_tab();
        let transition_key = self.transition_key();
        match self {
            AppRouter::Home => Shell::boxing(AppShell { active_tab }, move |ctx| {
                transitioned_page(transition_key, HomePage::boxing(ctx)).boxed()
            }),
            AppRouter::Blog => Shell::boxing(AppShell { active_tab }, move |ctx| {
                transitioned_page(transition_key, BlogListPage::boxing(ctx)).boxed()
            }),
            AppRouter::BlogDetail { id } => {
                let id = id.clone();
                Shell::boxing(AppShell { active_tab }, move |ctx| {
                    transitioned_page(transition_key, BlogDetailPage::boxing(id.clone(), ctx))
                        .boxed()
                })
            }
            AppRouter::Learn => Shell::boxing(AppShell { active_tab }, move |ctx| {
                transitioned_page(transition_key, LearnPage::boxing(ctx)).boxed()
            }),
            AppRouter::NotFound => Shell::boxing(AppShell { active_tab }, move |ctx| {
                let theme = ThemeData::of(ctx);
                transitioned_page(transition_key, not_found_page(*theme)).boxed()
            }),
        }
    }
}

/// A simple "page not found" placeholder rendered inside the shell content
/// area.
fn not_found_page(theme: ThemeData) -> impl Widget {
    Container::new()
        .color(theme.background_color)
        .child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Center)
                .vertical_alignment(BoxAlignment::Center)
                .children(vec![
                    Text::new("Page not found")
                        .text_align(TextAlign::MidCenter)
                        .text_style(
                            TextStyle::new()
                                .font_size(44)
                                .color(theme.on_background_color),
                        ),
                ]),
        )
}

#[cfg(test)]
mod tests {
    use aimer::router::Route;

    use super::*;

    #[test]
    fn routes_have_stable_distinct_transition_keys() {
        let keys = [
            AppRouter::Home.transition_key(),
            AppRouter::Blog.transition_key(),
            AppRouter::BlogDetail {
                id: "post".to_owned(),
            }
            .transition_key(),
            AppRouter::Learn.transition_key(),
            AppRouter::NotFound.transition_key(),
        ];

        assert_eq!(keys, ["home", "blog", "blog-detail", "learn", "not-found"]);
    }

    #[test]
    fn route_transition_uses_the_expected_duration() {
        assert_eq!(ROUTE_TRANSITION_DURATION, Duration::from_millis(200));
    }

    #[test]
    fn route_transition_has_stable_switcher_identity() {
        assert_eq!(
            Widget::key(&transitioned_page(
                "home",
                not_found_page(ThemeData::light())
            )),
            Some(Key::Value(ROUTE_SWITCHER_KEY.to_owned()))
        );
    }

    #[test]
    fn route_transition_erases_page_type_for_state_reuse() {
        let _: AnimatedSwitcher<Box<dyn Widget>> =
            transitioned_page("home", not_found_page(ThemeData::light()));
    }

    #[test]
    fn blog_detail_route_round_trips_and_keeps_the_blog_tab_active() {
        let route = AppRouter::BlogDetail {
            id: "introducing-aimer".to_owned(),
        };

        assert_eq!(route.format(), "/blog/introducing-aimer");
        assert_eq!(
            AppRouter::parse("/blog/introducing-aimer"),
            Some(route.clone())
        );
        assert_eq!(route.active_tab(), 1);
        assert_eq!(route.transition_key(), "blog-detail");
    }
}
