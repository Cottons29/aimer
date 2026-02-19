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

    let constructor_code = if !has_constructor {
        // Generate constructor code manually using original struct
        let struct_ts = quote! { #item_struct };
        widget_codegen::ConstructorCodegen::generate(struct_ts)
    } else {
        proc_macro2::TokenStream::new()
    };

    if !has_constructor {
        // Remove constructor attributes from the struct to avoid compilation errors
        // since we are not adding #[derive(Constructor)] which would handle them
        item_struct.attrs.retain(|attr| !attr.path().is_ident("constructor"));
        
        if let syn::Fields::Named(fields) = &mut item_struct.fields {
            for field in &mut fields.named {
                field.attrs.retain(|attr| !attr.path().is_ident("constructor"));
            }
        }
    }
    
    // Convert back to TokenStream for codegen
    let input_ts = quote! { #item_struct };

    let widget_code = if is_stateful {
        widget_codegen::StatefulWidgetCodegen::generate(input_ts)
    } else {
        widget_codegen::StatelessWidgetCodegen::generate(input_ts)
    };
    
    let final_output = quote! {
        #widget_code
        #constructor_code
    };
    
    proc_macro::TokenStream::from(final_output)
}
