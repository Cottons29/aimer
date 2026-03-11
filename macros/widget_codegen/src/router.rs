use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{parse2, ItemEnum, LitStr, Meta, Token, Expr, ExprArray, ExprLit, Lit, Fields};
use syn::punctuated::Punctuated;

pub struct RouterCodegen;

impl RouterCodegen {
    pub fn generate(input: TokenStream) -> TokenStream {
        let item_enum = match parse2::<ItemEnum>(input) {
            Ok(item) => item,
            Err(err) => return err.to_compile_error(),
        };

        let enum_name = &item_enum.ident;
        let mut parse_arms = Vec::new();
        let mut format_arms = Vec::new();

        for variant in &item_enum.variants {
            let variant_name = &variant.ident;
            let mut routes = Vec::new();

            for attr in &variant.attrs {
                if attr.path().is_ident("route") {
                    if let Ok(meta) = attr.parse_args::<LitStr>() {
                        routes.push(meta.value());
                    } else if let Meta::NameValue(mnv) = &attr.meta {
                        if let Expr::Lit(expr_lit) = &mnv.value {
                            if let Lit::Str(lit_str) = &expr_lit.lit {
                                routes.push(lit_str.value());
                            }
                        }
                    }
                } else if attr.path().is_ident("routes") {
                    if let Ok(meta) = attr.parse_args_with(Punctuated::<LitStr, Token![,]>::parse_terminated) {
                        for lit in meta {
                            routes.push(lit.value());
                        }
                    } else if let Meta::NameValue(mnv) = &attr.meta {
                        if let Expr::Array(ExprArray { elems, .. }) = &mnv.value {
                            for elem in elems {
                                if let Expr::Lit(ExprLit { lit: Lit::Str(lit_str), .. }) = elem {
                                    routes.push(lit_str.value());
                                }
                            }
                        }
                    } else if let Meta::List(ml) = &attr.meta {
                        if let Ok(expr_array) = syn::parse2::<ExprArray>(ml.tokens.clone()) {
                            for elem in expr_array.elems {
                                if let Expr::Lit(ExprLit { lit: Lit::Str(lit_str), .. }) = elem {
                                    routes.push(lit_str.value());
                                }
                            }
                        }
                    }
                }
            }

            if routes.is_empty() {
                routes.push(format!("/{}", variant_name.to_string().to_lowercase()));
            }

            let first_route = &routes[0];


            // Extract variable names from route (e.g., {id} or :id)
            // A simple implementation: assume segments like "{param}" mapping to struct fields or positional tuple fields.
            // This is complex for a generic script, but let's assume simple string interpolation for now.
            
            // For effort < 1.0, generating exact regex match logic
            let mut bind_pattern = quote! { Self::#variant_name };
            
            match &variant.fields {
                Fields::Named(fields) => {
                    let field_names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
                    bind_pattern = quote! { Self::#variant_name { #(#field_names),* } };
                    
                    // Format strings
                    // We simply use the route string and replace {field} with {}
                    // This is just a placeholder implementation
                    format_arms.push(quote! {
                        #bind_pattern => {
                            // Basic format string replace placeholder logic
                            let mut s = #first_route.to_string();
                            #(
                                s = s.replace(&format!("{{{}}}", stringify!(#field_names)), &#field_names.to_string());
                            )*
                            s
                        },
                    });
                },
                Fields::Unnamed(fields) => {
                    let field_names: Vec<_> = (0..fields.unnamed.len()).map(|i| format_ident!("arg_{}", i)).collect();
                    bind_pattern = quote! { Self::#variant_name( #(#field_names),* ) };
                    
                    // Simple logic for unnamed fields format
                    format_arms.push(quote! {
                        #bind_pattern => {
                            let mut s = #first_route.to_string();
                            s
                        },
                    });
                },
                Fields::Unit => {
                    format_arms.push(quote! {
                        Self::#variant_name => #first_route.to_string(),
                    });
                }
            }

            // For parse, generate basic string matches or segment parsing
            for route in routes {
                parse_arms.push(quote! {
                    if path == #route {
                        // Return unit variant if it's an exact match and has no fields
                        // For fields, this requires segment splitting logic
                    }
                });
            }
        }

        let first_variant = &item_enum.variants.first().unwrap().ident;

        quote! {
            #item_enum

            impl router::RouteParser<#enum_name> for #enum_name {
                fn parse(path: &str) -> #enum_name {
                    // Fallback stub for parse
                    // Advanced parsing logic using segment splits or regex should be generated here
                    #enum_name::#first_variant
                }

                fn format(route: &#enum_name) -> String {
                    match route {
                        #(#format_arms)*
                        _ => "/".to_string()
                    }
                }
            }
        }
    }
}
