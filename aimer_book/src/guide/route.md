# Router

The Aimer Router provides a powerful and safe way to manage navigation in your application. It follows a declarative approach inspired by some famous frameworks such as `Flutter` and `Svelte`, allowing you to define your routes and navigate between them using a simple API.

## 4.1 Defining Routes

### 4.1.1 Manual Implementation

To use the router, you first need to define a type that represents your application's routes. This is typically an `enum` that implements the `Route` trait.

The `Route` trait requires two methods:
- `parse`: Converts a string path (like from a browser URL) into your route type.
- `format`: Converts your route type into a string path.

```rust
use aimer::router::Route;

#[derive(Clone, Debug, PartialEq)]
pub enum AppRoute {
    Home,
    Settings,
    Profile(String), // Routes can have parameters
}

impl Route for AppRoute {
    fn parse(path: &str) -> Option<Self> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        match parts.as_slice() {
            [] => Some(AppRoute::Home),
            ["settings"] => Some(AppRoute::Settings),
            ["profile", id] => Some(AppRoute::Profile(id.to_string())),
            _ => None,
        }
    }

    fn format(&self) -> String {
        match self {
            AppRoute::Home => "/".to_string(),
            AppRoute::Settings => "/settings".to_string(),
            AppRoute::Profile(id) => format!("/profile/{}", id),
        }
    }
}
```

### 4.1.2 Using `#[widget(Router)]`

There are similar to `Manual Implementation` but you can use the `#[widget(Router)]` attribute to automatically generate the `parse` and `format` methods.

```rust
#[widget(Router)]
enum AppRoute {
    #[route("/")]
    Home,
    Settings, // no #[route] -> defaults to "/settings"
    #[route("/profile/{}")] // `{}` is a positional parameter for a tuple field
    Profile(String),
    #[route("/user/{id}")] // `{name}` binds to a named field
    User { id: String },
}
```

> **Placeholder syntax**: the macro uses brace placeholders — `{}` for unnamed
> (tuple) fields, and `{field}` for named struct fields — **not** the `:id`
> style. A variant with no `#[route(...)]` defaults to `/<variant_lowercase>`.

## 4.2 Mapping the Route to a Widget

To make the router work, you need to implement the `Router` trait for your route `enum`. This trait connects each route variant to the specific `Widget` that should be displayed.

The `Router` trait has one main method: `build(&self, ctx: &BuildContext)`. It works similarly to `StatelessWidget::build`, but it must return a `Box<dyn Widget>` because navigation often involves switching between different types of widgets at runtime.

```rust
use aimer::router::Router;
use aimer::Widget;
use aimer::BuildContext;

impl Router for AppRoute {
    fn build(&self, ctx: &BuildContext) -> Box<dyn Widget> {
        match self {
            AppRoute::Home => Box::new(HomeWidget {}),
            AppRoute::Settings => Box::new(SettingsPage {}),
            AppRoute::Profile(id) => Box::new(ProfilePage::new(id.clone())),
        }
    }
}
```

> **Note**: If you are using `#[widget(Router)]`, the macro automatically implements the `Widget` trait for your enum by calling `Router::build`. This allows your route enum to be used directly as a widget by the `Navigator`.

## 4.3 The Navigator

The `Navigator` is the heart of navigation. It maintains a stack of routes and builds the widget corresponding to the current top route.

```rust
use aimer::router::Navigator;
use aimer::Widget;

fn main_app() -> impl Widget {
    Navigator::new(AppRoute::Home, |route| {
        match route {
            AppRoute::Home => HomeWidget.into(),
            AppRoute::Settings => SettingsWidget.into(),
            AppRoute::Profile(id) => ProfileWidget::new(id).into(),
        }
    })
}
```

## 4.4 Navigating Between Routes

To navigate, you can use `NavigatorController::of(ctx)`. This gives you access to methods like `push` and `pop`.

```rust
use aimer::router::NavigatorController;
use aimer::BuildContext;

// Inside a build method or event handler:
fn on_click_settings(ctx: &BuildContext) {
    NavigatorController::<AppRoute>::of(ctx).push(AppRoute::Settings);
}

fn on_click_back(ctx: &BuildContext) {
    NavigatorController::<AppRoute>::of(ctx).pop();
}
```

### Navigation Methods:
- `push(route)`: Adds a new route to the stack and navigates to it.
- `pop()`: Removes the current route and goes back to the previous one.
- `can_pop()`: Returns `true` if there are previous routes in the stack.
- `history_len()`: Returns the number of routes in the stack.

## 4.5 Named Routes & Query Parameters

Routes can declare a stable `name` and carry query-string parameters, so you can
navigate by name with typed parameters instead of hand-building URLs.

```rust
#[widget(Router)]
#[derive(Clone)]
enum AppRoute {
    #[route("/")]
    Home,
    #[route("/profile/{name}", name = "profile")]
    Profile { name: String },
    // Query placeholders live after `?`; each `{field}` maps to a struct field.
    #[route("/search?q={q}&page={page}", name = "search")]
    Search { q: String, page: u32 },
}
```

- `Route::name()` returns the declared name (e.g. `Some("profile")`), or `None`.
- `format()` re-emits query parameters (sorted for determinism), e.g.
  `Search { q: "foo".into(), page: 2 }.format()` → `"/search?page=2&q=foo"`.
- `parse("/search?q=foo&page=2")` extracts both path and query parameters.

Navigate by name with a parameter map (keyed by field name):

```rust
use std::collections::HashMap;

let mut params = HashMap::new();
params.insert("name".to_string(), "alice".to_string());
NavigatorController::<AppRoute>::of(ctx).push_named("profile", &params);
```

`push_named` returns `true` when the name resolved and the route was pushed.

## 4.6 Redirects & Guards

Any route can define a guard/redirect hook that reroutes navigation before the
widget is built — perfect for auth gating. Attach `#[redirect(...)]` to a variant:

```rust
#[widget(Router)]
#[derive(Clone)]
enum AppRoute {
    #[route("/login", name = "login")]
    Login,
    #[route("/admin", name = "admin")]
    #[redirect(guard = "admin_guard")]
    Admin,
}

// A guard receives the route and context and returns `Some(other)` to reroute,
// or `None` to proceed.
fn admin_guard(_route: &AppRoute, ctx: &BuildContext) -> Option<AppRoute> {
    if is_authenticated(ctx) { None } else { Some(AppRoute::Login) }
}
```

You can also redirect to a fixed path with `#[redirect(to = "/login")]`.

Redirects are evaluated on initial load, on every `push`, and on browser
back/forward. The resolver follows the redirect chain until a route settles, and
is bounded by a max-hop guard (`MAX_REDIRECT_HOPS`) so redirect loops terminate
safely instead of hanging.

## 4.7 Nested Routes: Shell & Outlet

A **Shell** is a persistent layout frame (nav bar, drawer, header, ...) that
stays mounted while only an inner **Outlet** swaps between child routes. Declare
a nested route with `#[shell("/prefix")]` on a variant that embeds a child route
enum:

```rust
#[widget(Router)]
#[derive(Clone)]
enum AppRoute {
    #[route("/")]
    Home,
    #[shell("/dashboard", name = "dashboard")]
    Dashboard(DashRoute), // child enum, also #[widget(Router)]
}

#[widget(Router)]
#[derive(Clone)]
enum DashRoute {
    #[route("/")]
    Overview,   // -> /dashboard
    #[route("/reports")]
    Reports,    // -> /dashboard/reports
}
```

In `Router::build`, render a `Shell` whose frame contains an `Outlet`:

```rust
use aimer::router::{Shell, Outlet};

AppRoute::Dashboard(child) => {
    let child = child.clone();
    Shell::new(
        Container!(child: Outlet), // frame containing the Outlet
        move |_ctx| Box::new(child.clone()), // builds the active child
    ).boxed()
}
```

The shell injects the active child into the context; the descendant `Outlet`
reads it and renders it. An `Outlet` used without an ancestor `Shell` panics.

## 4.8 Tabbed Shells with Per-Branch History (`StatefulShell`)

A `StatefulShell` keeps an **independent navigation stack per branch**, so
switching tabs preserves each tab's history (like go_router's
`StatefulShellRoute`). Only the active branch's top route feeds the `Outlet`.

```rust
use aimer::router::{StatefulShell, StatefulShellController, Outlet};

fn tab_frame(_ctx: &BuildContext) -> Box<dyn Widget> {
    // A layout containing the Outlet plus (typically) a bottom nav bar whose
    // buttons call `StatefulShellController::<TabRoute>::of(ctx).go_branch(i)`.
    Box::new(Container!(child: Outlet))
}

fn tab_child(route: TabRoute) -> Box<dyn Widget> {
    Box::new(route)
}

StatefulShell::<TabRoute>::new(
    vec![TabRoute::Feed, TabRoute::Notifications, TabRoute::Profile], // one initial route per branch
    tab_frame,
    tab_child,
)
```

Control it via `StatefulShellController::<TabRoute>::of(ctx)`:

- `go_branch(index)`: switch the active branch (each branch keeps its own stack).
- `push_in_branch(index, route)`: push a route onto a specific branch's stack.
- `pop_in_branch(index)`: pop a branch's stack (guarded so it never empties).
- `active_branch()`: the current branch index.
- `branch_len(index)`: the stack depth of a branch.

Because branch stacks live in the shell's state, switching branches does **not**
rebuild sibling branches — their history and depth are restored intact when you
switch back.

---

## 4.9 Web/WASM Support

When targeting `wasm32`, the Aimer Router automatically integrates with the browser's History API:
- **Initial Route**: When the app starts, it attempts to parse the current browser URL to set the initial route.
- **Push/Pop**: Calling `push` or `pop` updates the browser's address bar and adds entries to the browser history.
- **Deep Linking**: Users can navigate directly to a path (e.g., `yourapp.com/settings`) and the app will load the correct route.



