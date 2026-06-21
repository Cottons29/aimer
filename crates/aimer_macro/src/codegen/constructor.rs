use crate::codegen::auto_wrapper::AutoWrapper;
use crate::field_info::FieldInfo;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Data, DataStruct, DeriveInput, Error, Fields, Ident};

pub fn constructor_derive(input: TokenStream, box_widget: bool) -> TokenStream {
    let input = match syn::parse2::<DeriveInput>(input) {
        Ok(data) => data,
        Err(err) => return err.to_compile_error(),
    };

    create_constructor(input, box_widget).unwrap_or_else(|err| err.to_compile_error())
}


fn create_constructor(ast: DeriveInput, box_widget: bool) -> Result<TokenStream, Error> {
    let name = &ast.ident;

    let mut crate_path: Option<TokenStream> = None;
    for attr in &ast.attrs {
        if attr.path().is_ident("constructor") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("crate") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    let path_str = value.value();
                    let path: syn::Path = syn::parse_str(&path_str)?;
                    crate_path = Some(quote! { #path });
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
        Data::Struct(DataStruct { fields: Fields::Named(fields), .. }) => &fields.named,
        _ => {
            return Err(Error::new(ast.span(), "Constructor can only be derived for structs with named fields"));
        }
    };

    let mut parsed_fields = Vec::new();
    for field in fields_named {
        parsed_fields.push(FieldInfo::parse_field_info(field)?);
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

    let constructor_fn = if box_widget {
        quote! {
            impl #impl_generics #name #ty_generics #where_clause {
                #[doc(hidden)]
                #[allow(clippy::too_many_arguments, dead_code)]
                pub fn create_new(#(#public_params),*) -> Box<dyn aimer_widget::Widget> {
                    Box::new(aimer_widget::NamedWidget::new(Box::new(Self {
                        #(#public_assigns,)*
                        #(#skipped_assigns,)*
                    }), stringify!(#name)))
                }
            }
        }
    } else {
        quote! {
            impl #impl_generics #name #ty_generics #where_clause {
                #[doc(hidden)]
                #[allow(clippy::too_many_arguments, dead_code)]
                pub fn create_new(#(#public_params),*) -> Self {
                    Self {
                        #(#public_assigns,)*
                        #(#skipped_assigns,)*
                    }
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
    let macro_name = Ident::new(&name.to_string(), proc_macro2::Span::call_site());
    // Macro Rule: Field Matcher
    // Matches: field_name: value
    let field_rules = public_fields.iter().map(|target| {
        let target_ident = target.ident;

        // Pattern matcher for current state
        let state_matcher: Vec<_> = public_fields
            .iter()
            .map(|f| {
                let f_ident = f.ident;
                let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                quote! { #f_ident : $#var_name:tt }
            })
            .collect();

        // State update logic
        let state_update = public_fields.iter().map(|f| {
            let f_ident = f.ident;
            if f_ident == target_ident {
                // If this is the target field, wrap the value
                let wrapper = AutoWrapper::new(target.ty);
                if target.into {
                    quote! { #f_ident : ((($val).into())) }
                } else if target.dyn_iter {
                    quote! {
                        #f_ident : ({
                            let mut temp_vec = Vec::new();
                            for item in $val {
                                temp_vec.push(Box::new(item) as _);
                            }
                            temp_vec
                        })
                    }
                } else {
                    let wrapped_expr = wrapper.wrap_expr(quote! { $val });
                    quote! { #f_ident : (#wrapped_expr) }
                }
            } else {
                // Otherwise keep old value
                let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                quote! { #f_ident : $#var_name }
            }
        });

        let array_rule = if target.dyn_iter || matches!(AutoWrapper::new(target.ty), AutoWrapper::Vec(_)) {
            // Determine the inner element type of the Vec (or dyn_iter field) to decide
            // whether items should be wrapped in Box::new(...) or passed through as-is.
            let vec_inner_ty = if let AutoWrapper::Vec(inner) = AutoWrapper::new(target.ty) { Some(inner) } else { None };
            #[allow(clippy::unnecessary_map_or)]
            let skip_box_wrap = target.dyn_iter
                || vec_inner_ty.as_ref().map_or(false, |inner| {
                    if let AutoWrapper::Terminal(ty) = inner.as_ref() {
                        AutoWrapper::is_widget_boxed(ty) || AutoWrapper::is_generic_widget_param(ty)
                    } else {
                        false
                    }
                });

            let state_update_array = public_fields.iter().map(|f| {
                let f_ident = f.ident;
                if f_ident == target_ident {
                    if skip_box_wrap {
                        quote! {
                            #f_ident : ({
                                vec![$($item,)*]
                            })
                        }
                    } else {
                        quote! {
                            #f_ident : ({
                                vec![$(Box::new($item),)*]
                            })
                        }
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

        // Generate async closure rules for fields with `into` attribute
        let async_rules = if target.into {
            let state_matcher_clone1: Vec<_> = public_fields
                .iter()
                .map(|f| {
                    let f_ident = f.ident;
                    let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                    quote! { #f_ident : $#var_name:tt }
                })
                .collect();
            let state_matcher_clone2 = state_matcher_clone1.clone();

            // State update for async move || { body }
            let state_update_async_move = public_fields.iter().map(|f| {
                let f_ident = f.ident;
                if f_ident == target_ident {
                    quote! { #f_ident : ((AsyncCallback(move || async move { $($body)* })).into()) }
                } else {
                    let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                    quote! { #f_ident : $#var_name }
                }
            });

            // State update for async || { body }
            let state_update_async = public_fields.iter().map(|f| {
                let f_ident = f.ident;
                if f_ident == target_ident {
                    quote! { #f_ident : ((AsyncCallback(|| async { $($body)* })).into()) }
                } else {
                    let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                    quote! { #f_ident : $#var_name }
                }
            });

            // Generate additional rules for async closures with arguments (async_wrapper)
            let async_arg_rules = if let Some(ref wrapper_name) = target.async_wrapper {
                let wrapper_ident = Ident::new(wrapper_name, target_ident.span());

                let state_matcher_arg1: Vec<_> = public_fields
                    .iter()
                    .map(|f| {
                        let f_ident = f.ident;
                        let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                        quote! { #f_ident : $#var_name:tt }
                    })
                    .collect();
                let state_matcher_arg2 = state_matcher_arg1.clone();

                // async move |arg| { body }
                let state_update_arg_move = public_fields.iter().map(|f| {
                    let f_ident = f.ident;
                    if f_ident == target_ident {
                        quote! { #f_ident : ((#wrapper_ident(move |$arg| async move { $($body)* })).into()) }
                    } else {
                        let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                        quote! { #f_ident : $#var_name }
                    }
                });

                // async |arg| { body }
                let state_update_arg = public_fields.iter().map(|f| {
                    let f_ident = f.ident;
                    if f_ident == target_ident {
                        quote! { #f_ident : ((#wrapper_ident(|$arg| async { $($body)* })).into()) }
                    } else {
                        let var_name = Ident::new(&format!("{}_old", f_ident), f_ident.span());
                        quote! { #f_ident : $#var_name }
                    }
                });

                quote! {
                    // async move |arg| { body }
                    (
                        @munch { #(#state_matcher_arg1),* }
                        #target_ident : async move |$arg:ident| { $($body:tt)* } $(, $($rest:tt)*)?
                    ) => {
                        #macro_name!(
                            @munch { #(#state_update_arg_move),* }
                            $($($rest)*)?
                        )
                    };

                    // async |arg| { body }
                    (
                        @munch { #(#state_matcher_arg2),* }
                        #target_ident : async |$arg:ident| { $($body:tt)* } $(, $($rest:tt)*)?
                    ) => {
                        #macro_name!(
                            @munch { #(#state_update_arg),* }
                            $($($rest)*)?
                        )
                    };
                }
            } else {
                quote! {}
            };

            quote! {
                #async_arg_rules

                // async move || { body }
                (
                    @munch { #(#state_matcher_clone1),* }
                    #target_ident : async move || { $($body:tt)* } $(, $($rest:tt)*)?
                ) => {
                    #macro_name!(
                        @munch { #(#state_update_async_move),* }
                        $($($rest)*)?
                    )
                };

                // async || { body }
                (
                    @munch { #(#state_matcher_clone2),* }
                    #target_ident : async || { $($body:tt)* } $(, $($rest:tt)*)?
                ) => {
                    #macro_name!(
                        @munch { #(#state_update_async),* }
                        $($($rest)*)?
                    )
                };
            }
        } else {
            quote! {}
        };

        quote! {
            #array_rule

            #async_rules

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
        let wrapper = AutoWrapper::new(f.ty);
        let val_wrapper = if f.into {
            quote! { ((($val).into())) }
        } else if f.dyn_iter {
            quote! {
                ({
                    let mut temp_vec = Vec::new();
                    for item in $val {
                        temp_vec.push(Box::new(item) as _);
                    }
                    temp_vec
                })
            }
        } else {
            wrapper.wrap_expr(quote! { $val })
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

        let array_rules = if f.dyn_iter || matches!(AutoWrapper::new(f.ty), AutoWrapper::Vec(_)) {
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

        let extras_str = if !extras.is_empty() { format!(" ({})", extras.join(", ")) } else { String::new() };

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
