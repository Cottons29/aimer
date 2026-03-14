use proc_macro::TokenStream;
use widget_codegen::ConstructorCodegen;

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
    let input : proc_macro2::TokenStream = proc_macro2::TokenStream::from(input);
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
///     pub fn create_new(label: String) -> Box<dyn oxidize::widget::Widget> {
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
    let input : proc_macro2::TokenStream = proc_macro2::TokenStream::from(input);
       ConstructorCodegen::generate_boxed(input).into()
}

