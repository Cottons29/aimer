use aimer::animation::{AnimatedSwitcher, Curve};
use aimer::router::{Route, Router, Shell, split_path_query};
use aimer::style::{TextAlign, TextStyle, Theme, ThemeData};
use aimer::*;
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

#[derive(Clone, Debug, PartialEq)]
pub enum AppRouter {
    Home,
    Blog,
    BlogDetail { id: String },
    Learn,
    NotFound,
}

impl Route for AppRouter {
    fn parse(full_path: &str) -> Option<Self> {
        let (path, _) = split_path_query(full_path);
        match path {
            "/" => Some(Self::Home),
            "/blog" => Some(Self::Blog),
            "/learn" => Some(Self::Learn),
            "/not-found" => Some(Self::NotFound),
            _ => path
                .strip_prefix("/blog/")
                .filter(|id| !id.contains('/'))
                .map(|id| Self::BlogDetail { id: id.to_owned() }),
        }
    }

    fn format(&self) -> String {
        match self {
            Self::Home => "/".to_owned(),
            Self::Blog => "/blog".to_owned(),
            Self::BlogDetail { id } => format!("/blog/{id}"),
            Self::Learn => "/learn".to_owned(),
            Self::NotFound => "/not-found".to_owned(),
        }
    }

    fn name(&self) -> Option<&'static str> {
        None
    }
}

impl Widget for AppRouter {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        Router::build(self, ctx).to_element(ctx)
    }
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

fn transitioned_page(key: &'static str, child: AnyWidget) -> AnimatedSwitcher<AnyWidget> {
    AnimatedSwitcher::new(ROUTE_TRANSITION_DURATION, Curve::FastOutSlowIn, child)
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
            AppRouter::Home => Shell::new(AppShell { active_tab }, move |ctx| {
                transitioned_page(transition_key, HomePage::boxing(ctx)).boxed()
            })
            .boxed(),
            AppRouter::Blog => Shell::new(AppShell { active_tab }, move |ctx| {
                transitioned_page(transition_key, BlogListPage::boxing(ctx)).boxed()
            })
            .boxed(),
            AppRouter::BlogDetail { id } => {
                let id = id.clone();
                Shell::new(AppShell { active_tab }, move |ctx| {
                    transitioned_page(transition_key, BlogDetailPage::boxing(id.clone(), ctx))
                        .boxed()
                })
                .boxed()
            }
            AppRouter::Learn => Shell::new(AppShell { active_tab }, move |ctx| {
                transitioned_page(transition_key, LearnPage::boxing(ctx)).boxed()
            })
            .boxed(),
            AppRouter::NotFound => Shell::new(AppShell { active_tab }, move |ctx| {
                let theme = ThemeData::of(ctx);
                transitioned_page(transition_key, not_found_page(*theme).boxed()).boxed()
            })
            .boxed(),
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
                not_found_page(ThemeData::light()).boxed()
            )),
            Some(Key::Value(ROUTE_SWITCHER_KEY.to_owned()))
        );
    }

    #[test]
    fn route_transition_erases_page_type_for_state_reuse() {
        let _: AnimatedSwitcher<AnyWidget> =
            transitioned_page("home", not_found_page(ThemeData::light()).boxed());
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

    #[test]
    fn static_routes_round_trip_and_ignore_query_parameters() {
        for route in [
            AppRouter::Home,
            AppRouter::Blog,
            AppRouter::Learn,
            AppRouter::NotFound,
        ] {
            assert_eq!(AppRouter::parse(&route.format()), Some(route));
        }
        assert_eq!(
            AppRouter::parse("/blog?source=header"),
            Some(AppRouter::Blog)
        );
    }

    #[test]
    fn blog_detail_route_rejects_additional_path_segments() {
        assert_eq!(AppRouter::parse("/blog/first/extra"), None);
    }
}
