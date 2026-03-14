use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, ItemStruct};

pub fn generate_raw_widget_impl(input: TokenStream) -> TokenStream {
    let item_struct = match parse2::<ItemStruct>(input.clone()) {
        Ok(s) => s,
        Err(_) => return input,
    };

    let struct_name = &item_struct.ident;
    let (impl_generics, ty_generics, where_clause) = item_struct.generics.split_for_impl();

    let output = quote! {
        #item_struct

        impl #impl_generics oxidize::widget::Widget for #struct_name #ty_generics #where_clause {
            fn to_element(&self, ctx: &oxidize::widget::base::BuildContext) -> Box<dyn oxidize::widget::Element> {
                unimplemented!("RawWidget: implement to_element for {}", stringify!(#struct_name))
            }
        }
    };

    output
}
