mod auto_trait_impl;
mod codegen;
mod unique_key;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Item, ItemFn, parse_macro_input};

use crate::auto_trait_impl::auto_impl;
use crate::codegen::router::RouterCodegen;
use crate::codegen::theme::{generate_theme_impl, style_path};
use crate::codegen::{RawWidgetCodegen, StatefulWidgetCodegen, StatelessWidgetCodegen};
use crate::unique_key::UniqueKeyInput;

/// Attribute macro that marks the Aimer application entry point.
///
/// Wraps the annotated function so it is callable from all supported targets:
/// native (via a `#[no_mangle] extern "C"` symbol), Android (via
/// `android_main`), and WebAssembly (via `#[wasm_bindgen]`).
///
/// # Usage
/// ```rust.ignore
///
/// #[aimer::main]
/// fn main() {
///     // application setup
/// }
/// ```
///
/// # What is generated
/// - The original function is kept as-is (marked `#[inline]`).
/// - **Native** (`not(target_arch = "wasm32")`): a `#[no_mangle] pub extern "C"
///   fn __generated_entrance_point()` that calls your function.
/// - **Android** (`target_os = "android"`): an `android_main(app: AndroidApp)`
///   that stores the `AndroidApp` handle in `ANDROID_APP` and then calls your
///   function.
/// - **WASM** (`target_arch = "wasm32"`): a `#[wasm_bindgen] pub fn
///   __generated_entrance_point()` that calls your function.
///
/// # Notes
/// - The macro does not accept any arguments; the `_attr` parameter is ignored.
/// - Your function must be a plain `fn` item (no async, no generics).
#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;

    let expanded = quote! {

        use aimer::wasm_bindgen;
        use aimer::wasm_bindgen::prelude::wasm_bindgen;
        #[inline]
        #input_fn

        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub extern "C" fn __generated_entrance_point(){
            #fn_name()
        }

        #[cfg(target_os = "android")]
        #[unsafe(no_mangle)]
        pub extern "C" fn android_main(app: aimer::quiver::winit::platform::android::activity::AndroidApp) {
            let _ = aimer::quiver::aimer_app::ANDROID_APP.set(app);
            #fn_name()
        }

        #[cfg(target_arch = "wasm32")]
        #[wasm_bindgen(start)]
        pub fn __generated_entrance_point(){
            #fn_name()
        }

    };

    TokenStream::from(expanded)
}

#[allow(dead_code)]
enum AttributeKind {
    Stateless,
    Stateful,
    Router,
    RawWidget,
}

impl TryFrom<&str> for AttributeKind {
    type Error = syn::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "stateless" => Ok(AttributeKind::Stateless),
            "stateful" => Ok(AttributeKind::Stateful),
            "router" => Ok(AttributeKind::Router),
            "rawwidget" => Ok(AttributeKind::RawWidget),
            _ => Err(syn::Error::new_spanned(
                value,
                "Only accepts `Stateless`, `Stateful`, `Router` or `RawWidget`",
            )),
        }
    }
}

/// Attribute macro that wires up a struct (or enum for `Router`) as a Widget.
///
/// Accepts one of four kinds as its argument:
///
/// | Kind | Target | What is generated |
/// |------|--------|-------------------|
/// | `Stateless` | struct | `impl Widget` via `StatelessWidget::build` |
/// | `Stateful` | struct | `impl Widget` via `StatefulElement` |
/// | `Router` | enum | full router `impl Widget` dispatch |
/// | `RawWidget` | struct | bare `impl Widget` stub (body uses `unimplemented!`) |
///
/// # Usage
/// ```rust,ignore
/// #[widget(Stateless)]
/// pub struct MyWidget {
///     pub label: String,
/// }
///
/// impl StatelessWidget for MyWidget {
///     fn build(&self, ctx: &BuildContext) -> AnyWidget {
///         // ...
///     }
/// }
/// ```
///
/// ```rust,ignore
/// #[widget(Router)]
/// pub enum AppRouter {
///     Home,
///     Settings,
/// }
/// ```
///
/// # Derive alternatives
/// The first three kinds also exist as derives —
/// [`StatelessWidget`](macro@StatelessWidget),
/// [`StatefulWidget`](macro@StatefulWidget) and [`Router`](macro@Router) —
/// which generate identical code. A derive is expanded *beside* the item, so
/// the definition the compiler sees is the one you wrote; this attribute
/// replaces it.
///
/// # Panics
/// Panics at compile time if no argument is provided.
#[proc_macro_attribute]
pub fn widget(args: TokenStream, input: TokenStream) -> TokenStream {
    if args.is_empty() {
        panic!("Missing the widget kind : Stateless, Stateful, Router or RawWidget");
    }

    let args_str = args.to_string();
    let is_stateful = args_str.to_lowercase().contains("stateful");
    let is_router = args_str.to_lowercase().contains("router");
    let is_raw_widget = args_str.to_lowercase().contains("rawwidget");

    // Parse the input item
    let item = parse_macro_input!(input as Item);

    if is_router {
        return if let Item::Enum(item_enum) = item {
            let input_ts = quote! { #item_enum };
            let router_code = RouterCodegen::generate(input_ts);
            TokenStream::from(router_code)
        } else {
            syn::Error::new_spanned(item, "Router widget can only be applied to enums")
                .to_compile_error()
                .into()
        };
    }

    let item_struct = match item {
        Item::Struct(s) => s,
        _ => {
            return syn::Error::new_spanned(
                item,
                "Widget attribute expects a struct unless using Router",
            )
            .to_compile_error()
            .into();
        }
    };

    // Convert back to TokenStream for codegen
    let input_ts = quote! { #item_struct };

    let widget_code = if is_raw_widget {
        RawWidgetCodegen::generate(input_ts)
    } else if is_stateful {
        StatefulWidgetCodegen::generate(input_ts)
    } else {
        StatelessWidgetCodegen::generate(input_ts)
    };

    TokenStream::from(widget_code)
}

/// Derives `Widget` for a stateless widget struct.
///
/// The derive form of [`#[widget(Stateless)]`](macro@widget): it generates the
/// exact same `Widget` impl, but is written *beside* your struct instead of
/// replacing it, so the definition the compiler sees is the one you wrote.
///
/// The generated `to_element` clones the widget and keeps the copy in a
/// [`StatelessElement`], so that the element can re-run `build()` when it is
/// marked dirty — on a resize, for instance. Your struct must therefore
/// implement both `Clone` and `StatelessWidget`.
///
/// A field named `key` (of type `Option<Key>`) is picked up automatically and
/// forwarded to the element, giving the widget a stable identity across
/// rebuilds.
///
/// # Examples
///
/// ```rust,ignore
/// use aimer::*;
///
/// #[derive(Clone, StatelessWidget)]
/// pub struct Greeting {
///     pub name: String,
/// }
///
/// impl StatelessWidget for Greeting {
///     fn build(&self, _: &BuildContext) -> AnyWidget {
///         Text::new(format!("Hello, {}", self.name)).boxed()
///     }
/// }
/// ```
///
/// [`StatelessElement`]: https://docs.rs/aimer_widget
#[proc_macro_derive(StatelessWidget)]
pub fn stateless_widget_derive(input: TokenStream) -> TokenStream {
    TokenStream::from(StatelessWidgetCodegen::derive(input.into()))
}

/// Derives `Widget` for a stateful widget struct.
///
/// The derive form of [`#[widget(Stateful)]`](macro@widget): it generates the
/// exact same `Widget` impl, but is written *beside* your struct instead of
/// replacing it, so the definition the compiler sees is the one you wrote.
///
/// The generated `to_element` hands the widget to a `StatefulElement`, which
/// owns the `State` your `StatefulWidget` impl creates and keeps it alive
/// across rebuilds. Your struct must implement `StatefulWidget`.
///
/// A field named `key` (of type `Option<Key>`) is picked up automatically and
/// forwarded to the element, giving the state a stable identity across
/// rebuilds.
///
/// # Examples
///
/// ```rust,ignore
/// use aimer::*;
///
/// #[derive(StatefulWidget)]
/// pub struct Counter {
///     pub initial_count: i32,
/// }
///
/// impl StatefulWidget for Counter {
///     type State = CounterState;
///
///     fn create_state(self) -> CounterState {
///         CounterState { count: self.initial_count }
///     }
/// }
/// ```
#[proc_macro_derive(StatefulWidget)]
pub fn stateful_widget_derive(input: TokenStream) -> TokenStream {
    TokenStream::from(StatefulWidgetCodegen::derive(input.into()))
}

/// Derives `Route` and `Widget` for a route enum.
///
/// The derive form of [`#[widget(Router)]`](macro@widget): it generates the
/// exact same `Route` parsing/formatting table and `Widget` dispatch, but is
/// written *beside* your enum instead of replacing it. You supply the
/// `Router::build` impl that turns a route into a widget.
///
/// # Helper attributes
///
/// | Attribute | On | Meaning |
/// |-----------|----|---------|
/// | `#[route("/path/{field}", name = "...")]` | variant | One path template. `{field}` binds a named field, `{}` a tuple field, and anything after `?` is read from the query string. An optional `name` makes the route addressable by [`Route::resolve_named`]. |
/// | `#[routes("/a", "/b")]` | variant | Several templates parsing to the same variant. The first one is what `format()` produces. |
/// | `#[shell("/prefix", name = "...")]` | variant | A nested route: the variant's single unnamed field is a child route enum, and the remainder of the path is parsed by it. |
/// | `#[redirect(guard = "path::to::fn")]` or `#[redirect(to = "/other")]` | variant | Overrides [`Route::redirect`] for this variant. |
///
/// A variant with no `#[route]`/`#[routes]` falls back to `/` plus its
/// lowercased name.
///
/// # Examples
///
/// ```rust,ignore
/// use aimer::*;
/// use aimer::router::Router;
///
/// #[derive(Clone, Debug, PartialEq, Router)]
/// pub enum AppRoute {
///     #[route("/")]
///     Home,
///     #[route("/profile/{name}", name = "profile")]
///     Profile { name: String },
/// }
///
/// impl Router for AppRoute {
///     fn build(&self, _: &BuildContext) -> AnyWidget {
///         match self {
///             AppRoute::Home => HomeScreen {}.boxed(),
///             AppRoute::Profile { name } => ProfileScreen::new(name).boxed(),
///         }
///     }
/// }
/// ```
///
/// [`Route::resolve_named`]: https://docs.rs/aimer_router
/// [`Route::redirect`]: https://docs.rs/aimer_router
#[proc_macro_derive(Router, attributes(route, routes, shell, redirect))]
pub fn router_derive(input: TokenStream) -> TokenStream {
    TokenStream::from(RouterCodegen::derive(input.into()))
}

#[proc_macro_derive(VisitorElement)]
pub fn visitor_element_derive(input: TokenStream) -> TokenStream {
    auto_impl("aimer_widget::VisitorElement", input)
}
#[proc_macro_derive(EventElement)]
pub fn event_element_derive(input: TokenStream) -> TokenStream {
    auto_impl("aimer_widget::EventElement", input)
}

#[proc_macro_derive(LayoutElement)]
pub fn layout_element_derive(input: TokenStream) -> TokenStream {
    auto_impl("aimer_widget::LayoutElement", input)
}

#[proc_macro_derive(Rebuildable)]
pub fn rebuildable_element_derive(input: TokenStream) -> TokenStream {
    auto_impl("aimer_widget::Rebuildable", input)
}

#[proc_macro_derive(Reconcilable)]
pub fn reconcilable_derive(input: TokenStream) -> TokenStream {
    auto_impl("aimer_widget::Reconcilable", input)
}

#[proc_macro_derive(Drawable)]
pub fn drawable_element_derive(input: TokenStream) -> TokenStream {
    auto_impl("aimer_widget::Drawable", input)
}

/// Derives field-by-field interpolation and theme lookup behavior for a named
/// struct.
#[proc_macro_derive(Theme)]
pub fn theme_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    let output = style_path().and_then(|style_path| generate_theme_impl(input, style_path));
    output.unwrap_or_else(syn::Error::into_compile_error).into()
}

#[proc_macro]
/// Generates a unique key for a widget that needs to remember its state.
pub fn key(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as UniqueKeyInput);

    let value = match input.prefix {
        Some(prefix) => format!("{}-{}", prefix.value(), uuid::Uuid::new_v4()),
        None => uuid::Uuid::new_v4().to_string(),
    };

    quote! {
        Key::Static(#value)
    }
    .into()
}
