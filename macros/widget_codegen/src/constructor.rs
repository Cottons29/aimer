use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    spanned::Spanned, Data, DataStruct, DeriveInput, Error, Expr, Field, Fields, Ident, Meta, Type,
    TypePath,
};

pub fn constructor_derive(input: TokenStream) -> TokenStream {
    let input = match syn::parse2::<DeriveInput>(input) {
        Ok(data) => data,
        Err(err) => return err.to_compile_error(),
    };

    match create_constructor(input) {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error(),
    }
}

struct FieldInfo<'a> {
    ident: &'a Ident,
    ty: &'a Type,
    skip: bool,
    default: Option<TokenStream>,
    into: bool,
    first: bool,
    dyn_iter: bool,
    docs: Vec<String>,
}

fn parse_field_info(field: &Field) -> Result<FieldInfo<'_>, Error> {
    let ident = field.ident.as_ref().ok_or_else(|| {
        Error::new(field.span(), "Constructor can only be derived for structs with named fields")
    })?;
    let ty = &field.ty;
    let mut skip = false;
    let mut default = None;
    let mut into = false;
    let mut first = false;
    let mut dyn_iter = false;
    let mut docs = Vec::new();

    for attr in &field.attrs {
        #[allow(clippy::collapsible_if)]
        if attr.path().is_ident("doc") {
            if let Meta::NameValue(meta) = &attr.meta {
                if let Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        docs.push(lit_str.value().trim().to_string());
                    }
                }
            }
        }

        if attr.path().is_ident("constructor") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    skip = true;
                    Ok(())
                } else if meta.path.is_ident("default") {
                    if meta.input.peek(syn::Token![=]) {
                        let value: Expr = meta.value()?.parse()?;
                        default = Some(quote! { #value });
                    } else {
                        default = Some(quote! { Default::default() });
                    }
                    Ok(())
                } else if meta.path.is_ident("into") {
                    into = true;
                    Ok(())
                } else if meta.path.is_ident("first") {
                    first = true;
                    Ok(())
                } else if meta.path.is_ident("dyn_iter") {
                    dyn_iter = true;
                    Ok(())
                } else if meta.path.is_ident("visibility") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    if value.value() == "private" {
                        skip = true;
                    }
                    Ok(())
                } else {
                    Err(meta.error("unsupported constructor attribute"))
                }
            })?;
        }
    }

    if default.is_none() && is_option(ty) {
        default = Some(quote! { None });
    }

    Ok(FieldInfo {
        ident,
        ty,
        skip,
        default,
        into,
        first,
        dyn_iter,
        docs,
    })
}

fn create_constructor(ast: DeriveInput) -> Result<TokenStream, Error> {
    let name = &ast.ident;
    
    let mut crate_path: Option<TokenStream> = None;
    for attr in &ast.attrs {
        if attr.path().is_ident("constructor") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    let path_str = value.value();
                    let path: syn::Path = syn::parse_str(&path_str)?;
                    crate_path = Some(quote!{ #path });
                    Ok(())
                } else {
                     Ok(())
                }
            })?;
        }
    }

    let struct_path = if let Some(p) = crate_path {
        quote! { #p :: #name }
    } else {
        quote! { #name }
    };

    let vis = &ast.vis;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let fields_named = match &ast.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => &fields.named,
        _ => {
            return Err(Error::new(
                ast.span(),
                "Constructor can only be derived for structs with named fields",
            ));
        }
    };

    let mut parsed_fields = Vec::new();
    for field in fields_named {
        parsed_fields.push(parse_field_info(field)?);
    }

    let public_fields: Vec<&FieldInfo> = parsed_fields.iter().filter(|f| !f.skip).collect();
    let skipped_fields: Vec<&FieldInfo> = parsed_fields.iter().filter(|f| f.skip).collect();

    // 1. Generate constructor function (fn __do__not__call__this__new)
    let public_params = public_fields.iter().map(|f| {
        let f_name = f.ident;
        let ty = f.ty;
        quote! { #f_name: #ty }
    });

    let public_assigns = public_fields.iter().map(|f| {
        let f_name = f.ident;
        quote! { #f_name }
    });

    let skipped_assigns = if !skipped_fields.is_empty() {
        let trait_name = Ident::new(&format!("{}Constructor", name), name.span());
        skipped_fields
            .iter()
            .map(|f| {
                let f_name = f.ident;
                quote! { #f_name: <#name #ty_generics as #trait_name #ty_generics>::#f_name() }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let constructor_fn = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            #[doc(hidden)]
            pub fn create_new(#(#public_params),*) -> Self {
                Self {
                    #(#public_assigns,)*
                    #(#skipped_assigns,)*
                }
            }
        }
    };

    // 2. Generate trait for skipped fields
    let trait_gen = if !skipped_fields.is_empty() {
        let trait_name = Ident::new(&format!("{}Constructor", name), name.span());
        let trait_methods = skipped_fields.iter().map(|f| {
            let f_name = f.ident;
            let ty = f.ty;
            quote! {
                fn #f_name() -> #ty;
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

    // 3. Generate Declarative Macro
    let macro_name = name.clone();
    // Macro Rule: Field Matcher
    // Matches: field_name: value
    let field_rules = public_fields.iter().map(|target| {
        let target_ident = target.ident;

        // Pattern matcher for current state
        let state_matcher: Vec<_> = public_fields.iter().map(|f| {
            let f_ident = f.ident;
            let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
            quote! { #f_ident : $#var_name:tt }
        }).collect();

        // State update logic
        let state_update = public_fields.iter().map(|f| {
            let f_ident = f.ident;
            if f_ident == target_ident {
                // If this is the target field, wrap the value
                if target.into {
                    quote! { #f_ident : ((($val).into())) }
                } else if target.dyn_iter || is_collection_of_box(target.ty) {
                    // let size = ;
                    quote! {
                        #f_ident : ({
                            let mut temp_vec = Vec::new();
                            for item in $val {
                                temp_vec.push(Box::new(item) as _);
                            }
                            temp_vec
                        })
                    }
                } else if is_option_of_box(target.ty) {
                    quote! { #f_ident : (Some(Box::new($val))) }
                } else if is_option_of_arc(target.ty) {
                    quote! { #f_ident : (Some(std::sync::Arc::new($val))) }
                } else if is_option(target.ty) {
                    quote! { #f_ident : (Some($val)) }
                } else if is_box(target.ty) {
                    quote! { #f_ident : (Box::new($val)) }
                } else if is_arc(target.ty) {
                    quote! { #f_ident : (std::sync::Arc::new($val)) }
                } else {
                    quote! { #f_ident : ($val) }
                }
            } else {
                // Otherwise keep old value
                let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                quote! { #f_ident : $#var_name }
            }
        });

        let array_rule = if target.dyn_iter || is_collection_of_box(target.ty) {
            let state_update_array = public_fields.iter().map(|f| {
                let f_ident = f.ident;
                if f_ident == target_ident {
                    quote! {
                        #f_ident : ({
                            vec![$(Box::new($item),)*]
                            // let mut temp_vec = Vec::new();
                            // $(
                            //     temp_vec.push(Box::new($item) as _);
                            // )*
                            // temp_vec
                        })
                    }
                } else {
                    let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                    quote! { #f_ident : $#var_name }
                }
            });

            quote! {
                (
                    @munch { #(#state_matcher),* }
                    #target_ident : [ $($item:expr),* $(,)? ] $(, $($rest:tt)*)?
                ) => {
                    #macro_name!(
                        @munch { #(#state_update_array),* }
                        $($($rest)*)?
                    )
                };
            }
        } else {
            quote! {}
        };

        quote! {
            #array_rule

            (
                @munch { #(#state_matcher),* }
                #target_ident : $val:expr $(, $($rest:tt)*)?
            ) => {
                #macro_name!(
                    @munch { #(#state_update),* }
                    $($($rest)*)?
                )
            };
        }
    });

    // Macro Rule: Success Matcher (All fields parsed)
    // Invokes the constructor function
    let success_matcher_fields = public_fields.iter().map(|f| {
        let ident = f.ident;
        quote! { #ident : ($#ident:expr) }
    });
    
    let call_args = public_fields.iter().map(|f| {
        let ident = f.ident;
        quote! { $#ident }
    });

    // Macro Rule: Missing Fields (End of recursion)
    // Handles default values or errors
    let missing_field_rules = public_fields.iter().map(|target| {
        let target_ident = target.ident;
        let default_val = &target.default;

        // State matcher to find the missing field (it will be ())
        let state_matcher = public_fields.iter().map(|f| {
            let f_ident = f.ident;
            if f_ident == target_ident {
                quote! { #f_ident : () }
            } else {
                quote! { #f_ident : $#f_ident:tt }
            }
        });

        if let Some(default_tokens) = default_val {
             // If field is missing but has default, apply default
             let state_update = public_fields.iter().map(|f| {
                let f_ident = f.ident;
                if f_ident == target_ident {
                    quote! { #f_ident : (#default_tokens) }
                } else {
                    quote! { #f_ident : $#f_ident }
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
            // Field is mandatory and missing -> Error
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

    // Logic for 'first' field
    let first_fields: Vec<&FieldInfo> = public_fields.iter().copied().filter(|f| f.first).collect();
    if first_fields.len() > 1 {
         return Err(Error::new(first_fields[1].ident.span(), "Only one field can be marked as 'first'"));
    }
    let first_field = first_fields.first().copied();

    // Macro Rule: Initial State
    // Initializes all fields to ()
    let init_state_fields = public_fields.iter().map(|f| {
        let ident = f.ident;
        quote! { #ident : () }
    });

    // Prepare entry points for 'first' field
    let first_field_rules = if let Some(f) = first_field {
         let f_ident = f.ident;
         // Logic to wrap value
         let val_wrapper = if f.into {
            quote! { ((($val).into())) }
         } else if f.dyn_iter || is_collection_of_box(f.ty) {
            quote! {
                ({
                    let mut temp_vec = Vec::new();
                    for item in $val {
                        temp_vec.push(Box::new(item) as _);
                    }
                    temp_vec
                })
            }
         } else if is_option(f.ty) {
            quote! { (Some($val)) }
         } else if is_box(f.ty) {
            quote! { (Box::new($val)) }
         } else if is_arc(f.ty) {
            quote! { (std::sync::Arc::new($val)) }
         } else {
            quote! { ($val) }
         };
         
         let init_state_with_first = public_fields.iter().map(|field| {
             let ident = field.ident;
             if ident == f_ident {
                 quote! { #ident : #val_wrapper }
             } else {
                 quote! { #ident : () }
             }
         });
         
         // Collect into a Vec to iterate multiple times
         let init_state_with_first: Vec<_> = init_state_with_first.collect();
         
         let array_rules = if f.dyn_iter || is_collection_of_box(f.ty) {
             let val_wrapper_array = quote! {
                 ({
                     let mut temp_vec = Vec::new();
                     $(
                         temp_vec.push(Box::new($item) as _);
                     )*
                     temp_vec
                 })
             };
             
             let init_state_with_first_array = public_fields.iter().map(|field| {
                 let ident = field.ident;
                 if ident == f_ident {
                     quote! { #ident : #val_wrapper_array }
                 } else {
                     quote! { #ident : () }
                 }
             });
             let init_state_with_first_array: Vec<_> = init_state_with_first_array.collect();

             quote! {
                ( [ $($item:expr),* $(,)? ], $($rest:tt)* ) => {
                    #macro_name!(@munch { #(#init_state_with_first_array),* } $($rest)*)
                };
                ( [ $($item:expr),* $(,)? ] ) => {
                    #macro_name!(@munch { #(#init_state_with_first_array),* })
                };
             }
         } else {
             quote! {}
         };

         quote! {
            #array_rules

            ( $val:expr, $($rest:tt)* ) => {
                #macro_name!(@munch { #(#init_state_with_first),* } $($rest)*)
            };
            ( $val:expr ) => {
                #macro_name!(@munch { #(#init_state_with_first),* })
            };
         }
    } else {
        quote! {}
    };

    // Documentation Generation
    let mut doc_msg = format!("Constructor for [`{}`].\n\nFields:\n", name);
    for field in &public_fields {
        let f_name = field.ident;
        let f_ty = field.ty;
        let f_ty_str = quote!(#f_ty).to_string();
        
        let mut extras = Vec::new();
        if field.first {
             extras.push("Positional");
        }
        if field.default.is_some() {
            extras.push("Optional");
        }
        
        let extras_str = if !extras.is_empty() {
             format!(" ({})", extras.join(", "))
        } else {
             String::new()
        };

        doc_msg.push_str(&format!("- `{}`: `{}`{}", f_name, f_ty_str, extras_str));
        
        if !field.docs.is_empty() {
             doc_msg.push_str("\n  - ");
             doc_msg.push_str(&field.docs.join("\n  - "));
        }
        doc_msg.push('\n');
    }

    let constructor_macro = quote! {
        #[doc = #doc_msg]
        #[macro_export]
        macro_rules! #macro_name {
            #(#field_rules)*
            
            (
                @munch { #(#success_matcher_fields),* }
            ) => {
                #struct_path::create_new(#(#call_args),*)
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

            #first_field_rules

            ( $($args:tt)* ) => {
                #macro_name!(@munch { #(#init_state_fields),* } $($args)*)
            };
        }
    };

    Ok(quote! {
        #constructor_fn
        #trait_gen
        #constructor_macro
    })
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

fn is_option_of_box(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return is_box(inner_ty);
                    }
                }
            }
        }
    }
    false
}

fn is_arc(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Arc" {
                 return true;
            }
        }
    }
    false
}

fn is_option_of_arc(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return is_arc(inner_ty);
                    }
                }
            }
        }
    }
    false
}

fn is_collection_of_box(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Vec" || segment.ident == "Array" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return is_box(inner_ty);
                    }
                }
            }
        }
    }
    false
}
