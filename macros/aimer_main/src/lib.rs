use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

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