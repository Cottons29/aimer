use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemStruct, parse2};

pub fn generate_stateless_widget_impl(input: TokenStream) -> TokenStream {
    let item_struct = match parse2::<ItemStruct>(input.clone()) {
        Ok(s) => s,
        Err(_) => return input, // Should handle error properly but returning input is safe fallback
    };

    let struct_name = &item_struct.ident;
    let (impl_generics, ty_generics, where_clause) = item_struct
        .generics
        .split_for_impl();

    let struct_name_str = struct_name.to_string();

    // Detect if the struct has a `key` field
    let has_key = item_struct.fields.iter().any(|f| {
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
            fn key(&self) -> Option<aimer_widget::key::Key> {
                self.key.clone()
            }
        }
    } else {
        quote! {}
    };

    let output = quote! {
        #item_struct

        impl #impl_generics aimer_widget::Widget for #struct_name #ty_generics #where_clause {
            #key_method

            fn to_element(&self, ctx: &aimer_widget::base::BuildContext) -> Box<dyn aimer_widget::Element> {
                use widget::StatelessWidget;
                // Assumes self implements StatelessWidget
                let child_widget = self.build(ctx);
                let child_element = aimer_widget::Widget::to_element(&child_widget, ctx);
                // Capture an owned copy of the widget so the element can re-run
                // `build()` (re-reading `MediaQuery`) when marked dirty on resize.
                // This requires the widget to be `Clone` (widgets are cheap,
                // immutable configuration, like Flutter's).
                let __rebuild_source = ::std::clone::Clone::clone(self);
                let __rebuild = move |ctx: &aimer_widget::base::BuildContext| -> Box<dyn aimer_widget::Element> {
                    use widget::StatelessWidget;
                    let child_widget = __rebuild_source.build(ctx);
                    aimer_widget::Widget::to_element(&child_widget, ctx)
                };
                Box::new(aimer_widget::StatelessElement::new(
                    child_element,
                    __rebuild,
                    #key_pass,
                    #struct_name_str,
                ))
            }
            fn debug_name(&self) -> &'static str {
                #struct_name_str
            }
        }
    };
    output
}
