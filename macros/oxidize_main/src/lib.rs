use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let block = &input_fn.block;

    let expanded = quote! {

        use oxidize::wasm_bindgen;
        use oxidize::wasm_bindgen::prelude::wasm_bindgen;
        #[inline]
        #input_fn

        #[cfg(not(target_arch = "wasm32"))]
        #[unsafe(no_mangle)]
        pub extern "C" fn __oxidize_generated_entrance_point(){
            #fn_name()
        }

        #[cfg(target_os = "android")]
        #[unsafe(no_mangle)]
        pub extern "C" fn android_main(app: oxidize::engine::winit::platform::android::activity::AndroidApp) {
            let _ = oxidize::engine::oxidize::ANDROID_APP.set(app);
            #fn_name()
        }

        #[cfg(target_arch = "wasm32")]
        #[wasm_bindgen]
        pub fn __oxidize_generated_entrance_point(){
            #fn_name()
        }
        
    };

    TokenStream::from(expanded)
}