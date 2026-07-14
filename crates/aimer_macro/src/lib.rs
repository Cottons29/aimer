mod auto_trait_impl;
mod codegen;
mod unique_key;

use crate::auto_trait_impl::auto_impl;
use crate::codegen::router::RouterCodegen;
use crate::codegen::{RawWidgetCodegen, StatefulWidgetCodegen, StatelessWidgetCodegen};
use crate::unique_key::UniqueKeyInput;
use proc_macro::TokenStream;
use quote::quote;
use syn::{Item, ItemFn, parse_macro_input};

/// Attribute macro that marks the Aimer application entry point.
///
/// Wraps the annotated function so it is callable from all supported targets:
/// native (via a `#[no_mangle] extern "C"` symbol), Android (via `android_main`),
/// and WebAssembly (via `#[wasm_bindgen]`).
///
/// # Usage
/// ```rust,ignore
/// use aimer::aimer_main;
///
/// #[aimer_main::main]
/// fn main() {
///     // application setup
/// }
/// ```
///
/// # What is generated
/// - The original function is kept as-is (marked `#[inline]`).
/// - **Native** (`not(target_arch = "wasm32")`): a `#[no_mangle] pub extern "C" fn __generated_entrance_point()` that calls your function.
/// - **Android** (`target_os = "android"`): an `android_main(app: AndroidApp)` that stores the
///   `AndroidApp` handle in `ANDROID_APP` and then calls your function.
/// - **WASM** (`target_arch = "wasm32"`): a `#[wasm_bindgen] pub fn __generated_entrance_point()` that calls your function.
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
///     fn build(&self, ctx: &BuildContext) -> Box<dyn Widget> {
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
