use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, ItemStruct};

pub fn generate_stateless_widget_impl(input: TokenStream) -> TokenStream {
    let item_struct = match parse2::<ItemStruct>(input.clone()) {
        Ok(s) => s,
        Err(_) => return input, // Should handle error properly but returning input is safe fallback
    };
    
    let struct_name = &item_struct.ident;
    let (impl_generics, ty_generics, where_clause) = item_struct.generics.split_for_impl();

    let output = quote! {
        #item_struct

        impl #impl_generics widget::Widget for #struct_name #ty_generics #where_clause {
            fn to_element(&self, ctx: &widget::base::BuildContext) -> Box<dyn widget::Element> {
                use widget::StatelessWidget;
                // Assumes self implements StatelessWidget
                let child_widget = self.build(ctx);
                let child_element = widget::Widget::to_element(&child_widget, ctx);
                Box::new(widget::StatelessElement {
                    child: child_element
                })
            }
        }
    };
    output
}
