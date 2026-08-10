//! The derive forms of `#[widget(...)]`, exercised the way a user writes them.
//!
//! The unit tests in `aimer_macro` compare token streams; these compile the
//! generated code against the real traits, which is the only way to catch a
//! path that resolves inside the macro crate but not at the call site.

use aimer::router::Route;
use aimer::*;

#[derive(Clone, StatelessWidget)]
struct Greeting {
    name: String,
}

impl StatelessWidget for Greeting {
    fn build(&self, _: &BuildContext) -> impl Widget {
        Text::new(format!("Hello, {}", self.name))
    }
}

#[derive(Clone, StatelessWidget)]
struct KeyedGreeting {
    key: Option<Key>,
}

impl StatelessWidget for KeyedGreeting {
    fn build(&self, _: &BuildContext) -> impl Widget {
        Text::new("keyed")
    }
}

#[derive(StatefulWidget)]
struct Counter {
    initial_count: i32,
}

struct CounterState {
    count: i32,
}

impl StatefulWidget for Counter {
    type State = CounterState;

    fn create_state(self) -> CounterState {
        CounterState {
            count: self.initial_count,
        }
    }
}

impl State<Counter> for CounterState {
    fn init_state(&mut self, _: StateUpdater<Self>) {}

    fn build(&self, _: &BuildContext) -> impl Widget {
        Text::new(format!("{}", self.count))
    }
}

#[derive(Clone, Debug, PartialEq, Router)]
enum AppRoute {
    #[route("/")]
    Home,
    #[route("/profile/{name}", name = "profile")]
    Profile { name: String },
    #[route("/search?q={q}&page={page}")]
    Search { q: String, page: u32 },
    #[shell("/dashboard")]
    Dashboard(DashRoute),
}

#[derive(Clone, Debug, PartialEq, Router)]
enum DashRoute {
    #[route("/")]
    Overview,
    #[route("/reports")]
    Reports,
}

impl router::Router for AppRoute {
    fn build(&self, _: &BuildContext) -> AnyWidget {
        Greeting {
            name: "route".to_string(),
        }
        .boxed()
    }
}

impl router::Router for DashRoute {
    fn build(&self, _: &BuildContext) -> AnyWidget {
        Greeting {
            name: "dash".to_string(),
        }
        .boxed()
    }
}

fn assert_widget<W: Widget>() {}

#[test]
fn the_stateless_derive_makes_the_struct_a_widget() {
    assert_widget::<Greeting>();

    let greeting = Greeting {
        name: "aimer".to_string(),
    };
    assert_eq!(Widget::debug_name(&greeting), "Greeting");
    assert!(Widget::key(&greeting).is_none());
}

#[test]
fn the_stateless_derive_forwards_a_key_field() {
    let keyed = KeyedGreeting {
        key: Some(Key::Static("greeting")),
    };

    assert_eq!(Widget::key(&keyed), Some(Key::Static("greeting")));
}

#[test]
fn the_stateful_derive_makes_the_struct_a_widget() {
    assert_widget::<Counter>();

    let counter = Counter { initial_count: 7 };
    assert_eq!(Widget::debug_name(&counter), "Counter");
    assert_eq!(counter.create_state().count, 7);
}

#[test]
fn the_router_derive_makes_the_enum_a_widget() {
    assert_widget::<AppRoute>();
    assert_widget::<DashRoute>();
}

#[test]
fn the_router_derive_parses_and_formats_paths() {
    assert_eq!(AppRoute::parse("/"), Some(AppRoute::Home));
    assert_eq!(
        AppRoute::parse("/profile/john"),
        Some(AppRoute::Profile {
            name: "john".to_string()
        })
    );
    assert_eq!(AppRoute::Home.format(), "/");
    assert_eq!(
        AppRoute::Profile {
            name: "john".to_string()
        }
        .format(),
        "/profile/john"
    );
}

#[test]
fn the_router_derive_reads_the_query_string() {
    assert_eq!(
        AppRoute::parse("/search?q=aimer&page=2"),
        Some(AppRoute::Search {
            q: "aimer".to_string(),
            page: 2,
        })
    );
}

#[test]
fn the_router_derive_delegates_a_shell_to_its_child_enum() {
    assert_eq!(
        AppRoute::parse("/dashboard/reports"),
        Some(AppRoute::Dashboard(DashRoute::Reports))
    );
    assert_eq!(
        AppRoute::Dashboard(DashRoute::Reports).format(),
        "/dashboard/reports"
    );
}

#[test]
fn the_router_derive_resolves_a_named_route() {
    let params = std::collections::HashMap::from([("name".to_string(), "john".to_string())]);

    assert_eq!(
        AppRoute::resolve_named("profile", &params),
        Some(AppRoute::Profile {
            name: "john".to_string()
        })
    );
    assert_eq!(
        AppRoute::Profile {
            name: "john".to_string()
        }
        .name(),
        Some("profile")
    );
}
