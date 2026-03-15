use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, parse_quote, Attribute, Item, ItemStruct, Path, Token};
use syn::punctuated::Punctuated;

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
    if args.is_empty()  {
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
            let router_code = widget_codegen::router::RouterCodegen::generate(input_ts);
            proc_macro::TokenStream::from(router_code)
        } else {
            syn::Error::new_spanned(item, "Router widget can only be applied to enums")
                .to_compile_error()
                .into()
        }
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
                list.iter().any(|path| {
                    if let Some(segment) = path.segments.last() {
                        segment.ident == "Constructor"
                    } else {
                        false
                    }
                })
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
        widget_codegen::ConstructorCodegen::generate_boxed(struct_ts)
    } else {
        proc_macro2::TokenStream::new()
    };

    if !has_constructor {
        // Remove constructor attributes from the struct to avoid compilation errors
        // since we are not adding #[derive(Constructor)] which would handle them
        item_struct.attrs.retain(|attr| !attr.path().is_ident("constructor"));

        if let syn::Fields::Named(fields) = &mut item_struct.fields {
            for field in &mut fields.named {
                field.attrs.retain(|attr| !attr.path().is_ident("constructor"));
            }
        }
    }

    // Convert back to TokenStream for codegen
    let input_ts = quote! { #item_struct };

    let widget_code = if is_raw_widget {
        widget_codegen::RawWidgetCodegen::generate(input_ts)
    } else if is_stateful {
        widget_codegen::StatefulWidgetCodegen::generate(input_ts)
    } else {
        widget_codegen::StatelessWidgetCodegen::generate(input_ts)
    };

    let final_output = quote! {
        #widget_code
        #constructor_code
    };

    proc_macro::TokenStream::from(final_output)
}


