use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let block = &input_fn.block;

    let expanded = quote! {
        // #input_fn

        #[unsafe(no_mangle)]
        pub extern "C" fn __oxidize_generated_entrance_point()
            #block
        
    };

    TokenStream::from(expanded)
}