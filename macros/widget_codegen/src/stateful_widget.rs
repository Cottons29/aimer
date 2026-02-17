use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, ItemStruct};

pub fn generate_stateful_widget_impl(input: TokenStream) -> TokenStream {
    let item_struct = match parse2::<ItemStruct>(input.clone()) {
        Ok(s) => s,
        Err(_) => return input,
    };
    
    let struct_name = &item_struct.ident;
    let (impl_generics, ty_generics, where_clause) = item_struct.generics.split_for_impl();

    let output = quote! {
        #item_struct

        impl #impl_generics widget::Widget for #struct_name #ty_generics #where_clause {
            fn to_element(&self, ctx: &widget::base::BuildContext) -> Box<dyn widget::Element> {
                use widget::{StatefulWidget, State};
                
                let state = self.create_state();
                let child_element = {
                    let child_widget = state.build();
                    widget::Widget::to_element(&child_widget, ctx)
                };
                
                Box::new(widget::StatefulElement {
                    child: child_element,
                    state: Box::new(state)
                })
            }
        }
    };
    
    output
}
