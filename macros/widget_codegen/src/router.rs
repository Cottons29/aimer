use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{parse2, ItemEnum, LitStr, Meta, Token, Expr, ExprArray, ExprLit, Lit, Fields};
use syn::punctuated::Punctuated;

pub struct RouterCodegen;

impl RouterCodegen {
    pub fn generate(input: TokenStream) -> TokenStream {
        let mut item_enum = match parse2::<ItemEnum>(input) {
            Ok(item) => item,
            Err(err) => return err.to_compile_error(),
        };

        let enum_name = &item_enum.ident;
        let mut parse_arms = Vec::new();
        let mut format_arms = Vec::new();

        for variant in &mut item_enum.variants {
            let variant_name = &variant.ident;
            let mut routes = Vec::new();

            // Extract routes and remove the attributes from the AST
            let mut new_attrs = Vec::new();
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
                } else {
                    new_attrs.push(attr.clone());
                }
            }
            variant.attrs = new_attrs;

            if routes.is_empty() {
                routes.push(format!("/{}", variant_name.to_string().to_lowercase()));
            }

            let first_route = &routes[0];


            // Extract variable names from route (e.g., {id} or :id)
            // A simple implementation: assume segments like "{param}" mapping to struct fields or positional tuple fields.
            // This is complex for a generic script, but let's assume simple string interpolation for now.
            
            // For effort < 1.0, generating exact regex match logic
            match &variant.fields {
                Fields::Named(fields) => {
                    let field_names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
                    let bind_pattern = quote! { Self::#variant_name { #(#field_names),* } };
                    
                    format_arms.push(quote! {
                        #bind_pattern => {
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
                    let bind_pattern = quote! { Self::#variant_name( #(#field_names),* ) };
                    
                    format_arms.push(quote! {
                        #bind_pattern => {
                            let mut s = #first_route.to_string();
                            #(
                                s = s.replacen("{}", &#field_names.to_string(), 1);
                            )*
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

            for route in &routes {
                let template_segments: Vec<&str> = route.split('/').collect();
                let n_segments = template_segments.len();

                let parse_arm = match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();

                        // For each field, find which segment index it corresponds to
                        let mut field_indices: Vec<usize> = Vec::new();
                        for field_name in &field_names {
                            let placeholder = format!("{{{}}}", field_name.as_ref().unwrap());
                            let idx = template_segments.iter().position(|s| *s == placeholder).unwrap_or(0);
                            field_indices.push(idx);
                        }

                        // Build static segment checks (non-placeholder segments)
                        let static_checks: Vec<_> = template_segments.iter().enumerate()
                            .filter(|(_, s)| !s.starts_with('{') || !s.ends_with('}'))
                            .map(|(i, s)| quote! { parts[#i] == #s })
                            .collect();

                        let field_extracts = field_names.iter().zip(field_indices.iter()).map(|(name, idx)| {
                            quote! {
                                let #name = parts[#idx].parse().ok()?;
                            }
                        });

                        quote! {
                            {
                                let parts: Vec<&str> = path.splitn(#n_segments + 1, '/').collect();
                                if parts.len() == #n_segments #( && #static_checks )* {
                                    #(#field_extracts)*
                                    return Some(Self::#variant_name { #(#field_names),* });
                                }
                            }
                        }
                    },
                    Fields::Unnamed(fields) => {
                        let n_fields = fields.unnamed.len();
                        // Find indices of `{}` placeholders
                        let placeholder_indices: Vec<usize> = template_segments.iter().enumerate()
                            .filter(|(_, s)| **s == "{}")
                            .map(|(i, _)| i)
                            .collect();

                        let static_checks: Vec<_> = template_segments.iter().enumerate()
                            .filter(|(_, s)| **s != "{}")
                            .map(|(i, s)| quote! { parts[#i] == #s })
                            .collect();

                        let arg_names: Vec<_> = (0..n_fields).map(|i| format_ident!("arg_{}", i)).collect();
                        let field_extracts = arg_names.iter().zip(placeholder_indices.iter()).map(|(name, idx)| {
                            quote! {
                                let #name = parts[#idx].parse().ok()?;
                            }
                        });

                        quote! {
                            {
                                let parts: Vec<&str> = path.splitn(#n_segments + 1, '/').collect();
                                if parts.len() == #n_segments #( && #static_checks )* {
                                    #(#field_extracts)*
                                    return Some(Self::#variant_name( #(#arg_names),* ));
                                }
                            }
                        }
                    },
                    Fields::Unit => {
                        quote! {
                            if path == #route {
                                return Some(Self::#variant_name);
                            }
                        }
                    }
                };

                parse_arms.push(parse_arm);
            }
        }

        let first_variant = &item_enum.variants.first().unwrap().ident;

        quote! {
            #item_enum

            impl router::Route for #enum_name {
                fn parse(path: &str) -> Option<#enum_name> {
                    #(#parse_arms)*
                    None
                }

                fn format(&self) -> String {
                    match self {
                        #(#format_arms)*
                        _ => "/".to_string()
                    }
                }
            }
        }
    }
}
