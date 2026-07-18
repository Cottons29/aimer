use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Path};

pub fn auto_impl(trait_name: &str, input: TokenStream) -> TokenStream {
    let input = match syn::parse2::<DeriveInput>(input.into()) {
        Ok(data) => data,
        Err(err) => {
            return err
                .to_compile_error()
                .into();
        }
    };

    let trait_path: Path = match syn::parse_str(trait_name) {
        Ok(path) => path,

        Err(err) => {
            return err
                .to_compile_error()
                .into();
        }
    };
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input
        .generics
        .split_for_impl();

    quote! {
        impl #impl_generics #trait_path for #name #ty_generics  #where_clause {}
    }
    .into()
}
