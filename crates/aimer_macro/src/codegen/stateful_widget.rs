use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemStruct, parse2};

pub fn generate_stateful_widget_impl(input: TokenStream) -> TokenStream {
    let item_struct = match parse2::<ItemStruct>(input.clone()) {
        Ok(s) => s,
        Err(_) => return input,
    };

    let struct_name = &item_struct.ident;
    let (impl_generics, ty_generics, where_clause) = item_struct
        .generics
        .split_for_impl();

    // Detect if the struct has a `key` field
    let has_key = item_struct
        .fields
        .iter()
        .any(|f| {
            f.ident
                .as_ref()
                .is_some_and(|i| i == "key")
        });

    let key_pass = if has_key {
        quote! { self.key.clone() }
    } else {
        quote! { None }
    };

    let key_method = if has_key {
        quote! {
            fn key(&self) -> Option<widget::key::Key> {
                self.key.clone()
            }
        }
    } else {
        quote! {}
    };

    let output = quote! {
        #item_struct

        impl #impl_generics widget::Widget for #struct_name #ty_generics #where_clause {
            #key_method

            fn to_element(&self, ctx: &widget::base::BuildContext) -> Box<dyn widget::Element> {
                let (element, _updater) = widget::StatefulElement::new_with_name(self, ctx, stringify!(#struct_name), #key_pass);
                Box::new(element)
            }
            fn debug_name(&self) -> &'static str {
                stringify!(#struct_name)
            }
        }
    };

    output
}
