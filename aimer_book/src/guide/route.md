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
    Settings,
    #[route("/profile/:id")] // Use :id as a parameter
    Profile(String),
}
```

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

---

## 4.5 Web/WASM Support

When targeting `wasm32`, the Aimer Router automatically integrates with the browser's History API:
- **Initial Route**: When the app starts, it attempts to parse the current browser URL to set the initial route.
- **Push/Pop**: Calling `push` or `pop` updates the browser's address bar and adds entries to the browser history.
- **Deep Linking**: Users can navigate directly to a path (e.g., `yourapp.com/settings`) and the app will load the correct route.



