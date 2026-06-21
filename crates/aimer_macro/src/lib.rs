mod codegen;
mod field_info;
mod auto_trait_impl;

use crate::auto_trait_impl::auto_impl;
use crate::codegen::router::RouterCodegen;
use crate::codegen::{ConstructorCodegen, RawWidgetCodegen, StatefulWidgetCodegen, StatelessWidgetCodegen};
use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Item, ItemFn, Path, Token};

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
        #[wasm_bindgen]
        pub fn __generated_entrance_point(){
            #fn_name()
        }

    };

    TokenStream::from(expanded)
}

/// Derives a `create_new(...)` constructor method for a struct, returning `Self`.
///
/// # Usage
/// ```rust,ignore
/// #[derive(Constructor)]
/// pub struct MyWidget {
///     pub label: String,
///     pub value: i32,
/// }
/// ```
///
/// This generates:
/// ```rust,ignore
/// impl MyWidget {
///     pub fn create_new(label: String, value: i32) -> Self { ... }
/// }
/// ```
/// and a corresponding declarative macro `MyWidget!(label: ..., value: ...)` for ergonomic construction.
///
/// # Field Attributes (`#[constructor(...)]`)
/// - `#[constructor(skip)]` — exclude the field from the constructor parameters; the struct must
///   implement the generated `MyWidgetConstructor` trait to supply a default value.
/// - `#[constructor(default)]` — use `Default::default()` for this field (skips it from params).
/// - `#[constructor(default = expr)]` — use a custom expression as the default value.
/// - `#[constructor(into)]` — accept `impl Into<T>` for this field's parameter.
/// - `#[constructor(first)]` — place this field's parameter first in the argument list.
/// - `#[constructor(dyn_iter)]` — accept a dynamic iterator for this field.
/// - `#[constructor(visibility = "private")]` — alias for `skip`.
///
/// # Struct Attributes (`#[constructor(...)]`)
/// - `#[constructor(crate = "path")]` — override the crate path used in the generated code.
///
/// # Incompatibility
/// Cannot be combined with `#[derive(WidgetConstructor)]` on the same struct.
#[proc_macro_derive(Constructor, attributes(constructor))]
pub fn constructor_derive(input: TokenStream) -> TokenStream {
    let input: proc_macro2::TokenStream = proc_macro2::TokenStream::from(input);
    ConstructorCodegen::generate(input).into()
}

/// Derives a `create_new(...)` constructor method for a struct, returning `Box<dyn Widget>`.
///
/// This is the widget-aware variant of [`Constructor`]. Use it when the struct implements
/// the `Widget` trait and you need a boxed return type suitable for widget trees.
///
/// # Usage
/// ```rust,ignore
/// #[derive(WidgetConstructor)]
/// pub struct MyWidget {
///     pub label: String,
/// }
/// ```
///
/// This generates:
/// ```rust,ignore
/// impl MyWidget {
///     pub fn create_new(label: String) -> Box<dyn aimer::widget::Widget> {
///         Box::new(Self { label })
///     }
/// }
/// ```
/// and a corresponding declarative macro `MyWidget!(label: ...)` for ergonomic construction.
///
/// # Field Attributes
/// Supports the same `#[constructor(...)]` field and struct attributes as [`Constructor`]:
/// `skip`, `default`, `default = expr`, `into`, `first`, `dyn_iter`, `visibility = "private"`.
///
/// # Incompatibility
/// Cannot be combined with `#[derive(Constructor)]` on the same struct.
#[proc_macro_derive(WidgetConstructor, attributes(constructor))]
pub fn widget_constructor_derive(input: TokenStream) -> TokenStream {
    let input: proc_macro2::TokenStream = proc_macro2::TokenStream::from(input);
    ConstructorCodegen::generate_boxed(input).into()
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
            _ => Err(syn::Error::new_spanned(value, "Only accepts `Stateless`, `Stateful`, `Router` or `RawWidget`")),
        }
    }
}

/// Attribute macro that wires up a struct (or enum for `Router`) as a Widget.
///
/// Accepts one of four kinds as its argument:
///
/// | Kind | Target | What is generated |
/// |------|--------|-------------------|
/// | `Stateless` | struct | `impl Widget` via `StatelessWidget::build` + boxed `create_new` constructor |
/// | `Stateful` | struct | `impl Widget` via `StatefulElement` + boxed `create_new` constructor |
/// | `Router` | enum | full router `impl Widget` dispatch + boxed `create_new` constructor |
/// | `RawWidget` | struct | bare `impl Widget` stub (body uses `unimplemented!`) + boxed `create_new` constructor (placeholder) |
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
/// # Constructor generation
/// Unless `#[derive(Constructor)]` is already present on the struct, this macro automatically
/// generates a `create_new(...)` method returning `Box<dyn Widget>` and a matching declarative
/// macro (same name as the struct/enum) for ergonomic construction.
/// Field-level `#[constructor(...)]` attributes (`skip`, `default`, `into`, `first`, `dyn_iter`,
/// `visibility`) are supported and stripped before compilation.
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

    let mut item_struct = match item {
        Item::Struct(s) => s,
        _ => {
            return syn::Error::new_spanned(item, "Widget attribute expects a struct unless using Router")
                .to_compile_error()
                .into();
        }
    };

    // Check if Constructor derive is already present
    let has_constructor = item_struct.attrs.iter().any(|attr| {
        if attr.path().is_ident("derive") {
            if let Ok(list) = attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated) {
                list.iter()
                    .any(|path| if let Some(segment) = path.segments.last() { segment.ident == "Constructor" } else { false })
            } else {
                false
            }
        } else {
            false
        }
    });

    let constructor_code = if !has_constructor {
        // Generate constructor code manually using original struct
        let struct_ts = quote! { #item_struct };
        ConstructorCodegen::generate_boxed(struct_ts)
    } else {
        proc_macro2::TokenStream::new()
    };

    if !has_constructor {
        // Remove constructor attributes from the struct to avoid compilation errors
        // since we are not adding #[derive(Constructor)] which would handle them
        item_struct
            .attrs
            .retain(|attr| !attr.path().is_ident("constructor"));

        if let syn::Fields::Named(fields) = &mut item_struct.fields {
            for field in &mut fields.named {
                field
                    .attrs
                    .retain(|attr| !attr.path().is_ident("constructor"));
            }
        }
    }

    // Convert back to TokenStream for codegen
    let input_ts = quote! { #item_struct };

    let widget_code = if is_raw_widget {
        RawWidgetCodegen::generate(input_ts)
    } else if is_stateful {
        StatefulWidgetCodegen::generate(input_ts)
    } else {
        StatelessWidgetCodegen::generate(input_ts)
    };

    let final_output = quote! {
        #widget_code
        #constructor_code
    };

    TokenStream::from(final_output)
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

#[proc_macro_derive(Drawable)]
pub fn drawable_element_derive(input: TokenStream) -> TokenStream {
    auto_impl("aimer_widget::Drawable", input)
}


