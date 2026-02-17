use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, parse_quote, Attribute, ItemStruct, Path, Token};
use syn::punctuated::Punctuated;

#[proc_macro_attribute]
pub fn widget(args: TokenStream, input: TokenStream) -> TokenStream {
    let args_str = args.to_string();
    let is_stateful = args_str.to_lowercase().contains("stateful");
    
    // Parse the input struct
    let mut item_struct = parse_macro_input!(input as ItemStruct);
    
    // Check if Constructor derive is already present
    let has_constructor = item_struct.attrs.iter().any(|attr| {
        if attr.path().is_ident("derive") {
            if let Ok(list) = attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated) {
                list.iter().any(|path| {
                    if let Some(segment) = path.segments.last() {
                        segment.ident == "Constructor"
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        } else {
            false
        }
    });

    if !has_constructor {
        // Add #[derive(Constructor)]
        let constructor_attr: Attribute = parse_quote!(#[derive(widget::Constructor)]);
        item_struct.attrs.push(constructor_attr);
    }
    
    // Convert back to TokenStream for codegen
    let input_ts = quote! { #item_struct };

    let output = if is_stateful {
        widget_codegen::StatefulWidgetCodegen::generate(input_ts)
    } else {
        widget_codegen::StatelessWidgetCodegen::generate(input_ts)
    };
    
    proc_macro::TokenStream::from(output)
}
