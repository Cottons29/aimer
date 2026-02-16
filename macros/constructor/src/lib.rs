use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Data, DataStruct, DeriveInput, Field, Fields, Ident, Meta, Type, TypePath,
    parse_macro_input,
};

#[proc_macro_derive(Constructor, attributes(constructor))]
pub fn constructor_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match create_constructor(input) {
        Ok(tokens) => tokens,
        Err(err) => panic!("{}", err),
    }
}

fn create_constructor(ast: DeriveInput) -> Result<TokenStream, String> {
    let name = &ast.ident;
    let vis = &ast.vis;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    // Extract struct fields
    let fields = match &ast.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => &fields.named,
        _ => {
            return Err(
                "Constructor can only be derived for structs with named fields".to_string(),
            );
        }
    };

    // Separate fields into public (in constructor) and private (trait-based)
    let mut public_fields = Vec::new();
    let mut private_fields = Vec::new();

    for field in fields {
        if is_private_field(field) {
            private_fields.push(field);
        } else {
            public_fields.push(field);
        }
    }

    // Generate constructor parameters for public fields
    let public_params = public_fields.iter().map(|field| {
        let field_name = &field.ident;
        let field_ty = &field.ty;
        quote! { #field_name: #field_ty }
    });

    // Generate field assignments for constructor
    let public_field_names = public_fields.iter().map(|field| {
        let field_name = &field.ident;
        quote! { #field_name }
    });

    let private_field_names = if !private_fields.is_empty() {
        let trait_name = Ident::new(&format!("{}Constructor", name), name.span());
        private_fields
            .iter()
            .map(|field| {
                let field_name = &field.ident;
                quote! { #field_name: <#name #ty_generics as #trait_name #ty_generics>::#field_name() }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // Generate constructor function
    let constructor_fn = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub fn __do__not__call__this__new(#(#public_params),*) -> Self {
                Self {
                    #(#public_field_names,)*
                    #(#private_field_names,)*
                }
            }
        }
    };

    // Generate trait if there are private fields
    let trait_gen = if !private_fields.is_empty() {
        let trait_name = Ident::new(&format!("{}Constructor", name), name.span());
        let trait_methods = private_fields.iter().map(|field| {
            let field_name = &field.ident;
            let field_ty = &field.ty;
            quote! {
                fn #field_name() -> #field_ty;
            }
        });

        quote! {
            #vis trait #trait_name #impl_generics #where_clause {
                #(#trait_methods)*
            }
        }
    } else {
        quote! {}
    };

    // Generate declarative macro for construction
    let macro_name = name.clone();

    // 1. Field Rules
    let field_rules = public_fields.iter().map(|target_field| {
        let target_ident = target_field.ident.as_ref().expect("Named fields have idents");
        
        let state_matcher_fields = public_fields.iter().map(|f| {
            let f_name = f.ident.as_ref().expect("Named fields have idents");
            let var_name = Ident::new(&format!("{}_old", f_name), f_name.span());
            quote! { #f_name : $#var_name:tt }
        });
        
        let state_update_fields = public_fields.iter().map(|f| {
            let f_name = f.ident.as_ref().expect("Named fields have idents");
            if f_name == target_ident {
                if is_option(&target_field.ty) {
                    quote! { #f_name : (Some($val)) }
                } else if is_box(&target_field.ty) {
                    quote! { #f_name : (Box::new($val)) }
                } else {
                    quote! { #f_name : ($val) }
                }
            } else {
                 let var_name = Ident::new(&format!("{}_old", f_name), f_name.span());
                 quote! { #f_name : $#var_name }
            }
        });
        
        quote! {
            (
                @munch { #(#state_matcher_fields),* }
                #target_ident : $val:expr $(, $($rest:tt)*)?
            ) => {
                #macro_name!(
                    @munch { #(#state_update_fields),* }
                    $($($rest)*)?
                )
            };
        }
    });

    // 2. Success Rule
    let success_matcher_fields = public_fields.iter().map(|f| {
        let f_name = f.ident.as_ref().expect("Named fields have idents");
        quote! { #f_name : ($#f_name:expr) }
    });
    let call_args = public_fields.iter().map(|f| {
        let f_name = f.ident.as_ref().expect("Named fields have idents");
        quote! { $#f_name }
    });

    // 3. Missing Field Rules
    let missing_field_rules = public_fields.iter().map(|target_field| {
        let target_ident = target_field.ident.as_ref().expect("Named fields have idents");
        let is_opt = is_option(&target_field.ty);

        let state_matcher = public_fields.iter().map(|f| {
            let f_name = f.ident.as_ref().expect("Named fields have idents");
            if f_name == target_ident {
                quote! { #f_name : () }
            } else {
                quote! { #f_name : $#f_name:tt } 
            }
        });
        
        if is_opt {
             let state_update = public_fields.iter().map(|f| {
                let f_name = f.ident.as_ref().expect("Named fields have idents");
                if f_name == target_ident {
                    quote! { #f_name : (None) }
                } else {
                    quote! { #f_name : $#f_name } 
                }
            });
            quote! {
                (
                    @munch { #(#state_matcher),* }
                ) => {
                    #macro_name!(
                        @munch { #(#state_update),* }
                    )
                };
            }
        } else {
            let err_msg = format!("Missing field '{}'", target_ident);
            quote! {
                (
                    @munch { #(#state_matcher),* }
                ) => {
                    compile_error!(#err_msg)
                };
            }
        }
    });

    // 4. Initial State
    let init_state_fields = public_fields.iter().map(|f| {
        let f_name = f.ident.as_ref().expect("Named fields have idents");
        quote! { #f_name : () }
    });

    // Generate doc comment
    let mut doc_msg = format!("Constructor for [`{}`].\n\nFields:\n", name);
    for field in &public_fields {
        let f_name = field.ident.as_ref().expect("Named fields have idents");
        let f_ty = &field.ty;
        let f_ty_str = quote!(#f_ty).to_string();
        doc_msg.push_str(&format!("- `{}`: `{}`\n", f_name, f_ty_str));
    }

    let constructor_macro = quote! {
        #[doc = #doc_msg]
        #[macro_export]
        macro_rules! #macro_name {
            #(#field_rules)*
            
            (
                @munch { #(#success_matcher_fields),* }
            ) => {
                #name::__do__not__call__this__new(#(#call_args),*)
            };
            
            #(#missing_field_rules)*
            
            (
                @munch { $($state:tt)* }
                $field:ident : $val:expr $(, $($rest:tt)*)?
            ) => {
                compile_error!(concat!("Unknown field: ", stringify!($field)))
            };
            
            (
                @munch { $($state:tt)* }
                $($rest:tt)*
            ) => {
                compile_error!(concat!("Stuck on: ", stringify!($($rest)*)))
            };

            ( $($args:tt)* ) => {
                #macro_name!(@munch { #(#init_state_fields),* } $($args)*)
            };
        }
    };

    // Generate the output
    let output = quote! {
        #constructor_fn

        #trait_gen

        #constructor_macro
    };

    Ok(output.into())
}

fn is_private_field(field: &Field) -> bool {
    for attr in &field.attrs {
        if attr.path().is_ident("constructor") {
            if let Meta::List(meta_list) = &attr.meta {
                let tokens = meta_list.tokens.to_string();
                if tokens.contains("visibility") && tokens.contains("private") {
                    return true;
                }
            }
        }
    }
    false
}

fn is_option(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Option" {
                 return true;
            }
        }
    }
    false
}

fn is_box(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Box" {
                 return true;
            }
        }
    }
    false
}
