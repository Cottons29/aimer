use proc_macro::TokenStream;
use widget_codegen::ConstructorCodegen;


#[proc_macro_derive(Constructor, attributes(constructor))]
pub fn constructor_derive(input: TokenStream) -> TokenStream {
    let input : proc_macro2::TokenStream = proc_macro2::TokenStream::from(input);
       ConstructorCodegen::generate(input).into()
}

