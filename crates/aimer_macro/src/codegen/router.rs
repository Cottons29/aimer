use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{Expr, ExprArray, ExprLit, Fields, ItemEnum, Lit, LitStr, Meta, Token, parse2};

pub struct RouterCodegen;

/// Split a route template into its path portion and an optional query portion.
/// `"/user/{id}?tab={tab}"` -> `("/user/{id}", Some("tab={tab}"))`.
fn split_template(tpl: &str) -> (String, Option<String>) {
    match tpl.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q.to_string())),
        None => (tpl.to_string(), None),
    }
}

/// Parse a query template into `(key, value)` pairs where the value is usually
/// a `{placeholder}`. `"q={q}&page={page}"` -> `[("q","{q}"),
/// ("page","{page}")]`.
fn query_pairs(query: &Option<String>) -> Vec<(String, String)> {
    let Some(query) = query else {
        return Vec::new();
    };
    query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => (pair.to_string(), String::new()),
        })
        .collect()
}

impl RouterCodegen {
    pub fn generate(input: TokenStream) -> TokenStream {
        let mut item_enum = match parse2::<ItemEnum>(input) {
            Ok(item) => item,
            Err(err) => return err.to_compile_error(),
        };

        let enum_name = &item_enum.ident;
        let mut parse_arms = Vec::new();
        let mut format_arms = Vec::new();
        let mut name_arms = Vec::new();
        let mut resolve_arms = Vec::new();
        let mut redirect_arms = Vec::new();

        for variant in &mut item_enum.variants {
            let variant_name = &variant.ident;
            let mut routes = Vec::new();
            let mut name_opt: Option<String> = None;
            let mut redirect_guard: Option<String> = None;
            let mut redirect_to: Option<String> = None;
            let mut is_shell = false;
            let mut shell_prefix: Option<String> = None;

            // Extract routes/name and remove the attributes from the AST
            let mut new_attrs = Vec::new();
            #[allow(clippy::collapsible_if)]
            for attr in &variant.attrs {
                if attr.path().is_ident("route") {
                    if let Ok(exprs) =
                        attr.parse_args_with(Punctuated::<Expr, Token![,]>::parse_terminated)
                    {
                        for expr in exprs {
                            match expr {
                                Expr::Lit(ExprLit {
                                    lit: Lit::Str(lit_str),
                                    ..
                                }) => {
                                    routes.push(lit_str.value());
                                }
                                Expr::Assign(assign) => {
                                    if let Expr::Path(ep) = &*assign.left {
                                        if ep.path.is_ident("name") {
                                            if let Expr::Lit(ExprLit {
                                                lit: Lit::Str(lit_str),
                                                ..
                                            }) = &*assign.right
                                            {
                                                name_opt = Some(lit_str.value());
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else if let Meta::NameValue(mnv) = &attr.meta {
                        if let Expr::Lit(expr_lit) = &mnv.value {
                            if let Lit::Str(lit_str) = &expr_lit.lit {
                                routes.push(lit_str.value());
                            }
                        }
                    }
                } else if attr.path().is_ident("routes") {
                    if let Ok(meta) =
                        attr.parse_args_with(Punctuated::<LitStr, Token![,]>::parse_terminated)
                    {
                        for lit in meta {
                            routes.push(lit.value());
                        }
                    } else if let Meta::NameValue(mnv) = &attr.meta {
                        if let Expr::Array(ExprArray { elems, .. }) = &mnv.value {
                            for elem in elems {
                                if let Expr::Lit(ExprLit {
                                    lit: Lit::Str(lit_str),
                                    ..
                                }) = elem
                                {
                                    routes.push(lit_str.value());
                                }
                            }
                        }
                    } else if let Meta::List(ml) = &attr.meta {
                        if let Ok(expr_array) = parse2::<ExprArray>(ml.tokens.clone()) {
                            for elem in expr_array.elems {
                                if let Expr::Lit(ExprLit {
                                    lit: Lit::Str(lit_str),
                                    ..
                                }) = elem
                                {
                                    routes.push(lit_str.value());
                                }
                            }
                        }
                    }
                } else if attr.path().is_ident("shell") {
                    is_shell = true;
                    if let Ok(exprs) =
                        attr.parse_args_with(Punctuated::<Expr, Token![,]>::parse_terminated)
                    {
                        for expr in exprs {
                            match expr {
                                Expr::Lit(ExprLit {
                                    lit: Lit::Str(lit_str),
                                    ..
                                }) => {
                                    shell_prefix = Some(lit_str.value());
                                }
                                Expr::Assign(assign) => {
                                    if let Expr::Path(ep) = &*assign.left {
                                        if ep.path.is_ident("name") {
                                            if let Expr::Lit(ExprLit {
                                                lit: Lit::Str(lit_str),
                                                ..
                                            }) = &*assign.right
                                            {
                                                name_opt = Some(lit_str.value());
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                } else if attr
                    .path()
                    .is_ident("redirect")
                {
                    if let Ok(metas) =
                        attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                    {
                        for m in metas {
                            if let Meta::NameValue(nv) = m {
                                if nv.path.is_ident("guard") {
                                    if let Expr::Lit(ExprLit {
                                        lit: Lit::Str(s), ..
                                    }) = &nv.value
                                    {
                                        redirect_guard = Some(s.value());
                                    }
                                } else if nv.path.is_ident("to") {
                                    if let Expr::Lit(ExprLit {
                                        lit: Lit::Str(s), ..
                                    }) = &nv.value
                                    {
                                        redirect_to = Some(s.value());
                                    }
                                }
                            }
                        }
                    }
                } else {
                    new_attrs.push(attr.clone());
                }
            }
            variant.attrs = new_attrs;

            // ---- shell variant: nested route delegating to a child enum ----
            if is_shell {
                let prefix = shell_prefix
                    .clone()
                    .unwrap_or_else(|| {
                        format!(
                            "/{}",
                            variant_name
                                .to_string()
                                .to_lowercase()
                        )
                    });
                match &variant.fields {
                    Fields::Unnamed(f) if f.unnamed.len() == 1 => {
                        let child_ty = &f.unnamed.first().unwrap().ty;
                        let prefix_slash = format!("{}/", prefix);

                        format_arms.push(quote! {
                            Self::#variant_name(child) => {
                                let c = router::Route::format(child);
                                if c == "/" { #prefix.to_string() } else { format!("{}{}", #prefix, c) }
                            },
                        });

                        parse_arms.push(quote! {
                            {
                                if path == #prefix || path.starts_with(#prefix_slash) {
                                    let rem = &path[#prefix.len()..];
                                    let rem = if rem.is_empty() { "/" } else { rem };
                                    if let Some(child) = <#child_ty as router::Route>::parse(rem) {
                                        return Some(Self::#variant_name(child));
                                    }
                                }
                            }
                        });

                        if let Some(name) = &name_opt {
                            name_arms.push(quote! { Self::#variant_name(..) => Some(#name), });
                        }
                    }
                    _ => {
                        return syn::Error::new_spanned(
                            &variant.fields,
                            "#[shell] requires exactly one unnamed field: the child route enum",
                        )
                        .to_compile_error();
                    }
                }
                continue;
            }

            if routes.is_empty() {
                routes.push(format!(
                    "/{}",
                    variant_name
                        .to_string()
                        .to_lowercase()
                ));
            }

            let first_route = &routes[0];
            let (path_tpl, query_tpl) = split_template(first_route);
            let qpairs = query_pairs(&query_tpl);

            // ---- format() ----
            match &variant.fields {
                Fields::Named(fields) => {
                    let field_names: Vec<_> = fields
                        .named
                        .iter()
                        .map(|f| &f.ident)
                        .collect();
                    let bind_pattern = quote! { Self::#variant_name { #(#field_names),* } };

                    let mut path_replaces = Vec::new();
                    let mut query_pushes = Vec::new();
                    for field in fields.named.iter() {
                        let fname = field.ident.as_ref().unwrap();
                        let placeholder = format!("{{{}}}", fname);
                        if let Some((key, _)) = qpairs
                            .iter()
                            .find(|(_, v)| *v == placeholder)
                        {
                            query_pushes
                                .push(quote! { __q.push((#key.to_string(), #fname.to_string())); });
                        } else {
                            path_replaces
                                .push(quote! { s = s.replace(#placeholder, &#fname.to_string()); });
                        }
                    }

                    format_arms.push(quote! {
                        #bind_pattern => {
                            let mut s = #path_tpl.to_string();
                            #(#path_replaces)*
                            let mut __q: Vec<(String, String)> = Vec::new();
                            #(#query_pushes)*
                            s.push_str(&router::format_query_string(&__q));
                            s
                        },
                    });
                }
                Fields::Unnamed(fields) => {
                    let field_names: Vec<_> = (0..fields.unnamed.len())
                        .map(|i| format_ident!("arg_{}", i))
                        .collect();
                    let bind_pattern = quote! { Self::#variant_name( #(#field_names),* ) };

                    format_arms.push(quote! {
                        #bind_pattern => {
                            let mut s = #path_tpl.to_string();
                            #(
                                s = s.replacen("{}", &#field_names.to_string(), 1);
                            )*
                            s
                        },
                    });
                }
                Fields::Unit => {
                    format_arms.push(quote! {
                        Self::#variant_name => #path_tpl.to_string(),
                    });
                }
            }

            // ---- name() ----
            if let Some(name) = &name_opt {
                let pat = match &variant.fields {
                    Fields::Named(_) => quote! { Self::#variant_name { .. } },
                    Fields::Unnamed(_) => quote! { Self::#variant_name(..) },
                    Fields::Unit => quote! { Self::#variant_name },
                };
                name_arms.push(quote! { #pat => Some(#name), });

                // ---- resolve_named() ----
                match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names: Vec<_> = fields
                            .named
                            .iter()
                            .map(|f| &f.ident)
                            .collect();
                        let extracts = fields.named.iter().map(|f| {
                            let fname = f.ident.as_ref().unwrap();
                            quote! {
                                let #fname = params.get(stringify!(#fname)).and_then(|v| v.parse().ok())?;
                            }
                        });
                        resolve_arms.push(quote! {
                            #name => {
                                #(#extracts)*
                                return Some(Self::#variant_name { #(#field_names),* });
                            }
                        });
                    }
                    Fields::Unnamed(fields) => {
                        let arg_names: Vec<_> = (0..fields.unnamed.len())
                            .map(|i| format_ident!("arg_{}", i))
                            .collect();
                        let extracts = arg_names
                            .iter()
                            .enumerate()
                            .map(|(i, name)| {
                                let key = i.to_string();
                                quote! {
                                    let #name = params.get(#key).and_then(|v| v.parse().ok())?;
                                }
                            });
                        resolve_arms.push(quote! {
                            #name => {
                                #(#extracts)*
                                return Some(Self::#variant_name( #(#arg_names),* ));
                            }
                        });
                    }
                    Fields::Unit => {
                        resolve_arms.push(quote! {
                            #name => { return Some(Self::#variant_name); }
                        });
                    }
                }
            }

            // ---- redirect() ----
            if redirect_guard.is_some() || redirect_to.is_some() {
                let pat = match &variant.fields {
                    Fields::Named(_) => quote! { Self::#variant_name { .. } },
                    Fields::Unnamed(_) => quote! { Self::#variant_name(..) },
                    Fields::Unit => quote! { Self::#variant_name },
                };
                if let Some(guard) = &redirect_guard {
                    if let Ok(guard_path) = syn::parse_str::<syn::Path>(guard) {
                        redirect_arms.push(quote! { #pat => #guard_path(self, ctx), });
                    }
                } else if let Some(to) = &redirect_to {
                    redirect_arms.push(quote! { #pat => <Self as router::Route>::parse(#to), });
                }
            }

            // ---- parse() ----
            for route in &routes {
                let (route_path, route_query) = split_template(route);
                let route_qpairs = query_pairs(&route_query);
                let template_segments: Vec<&str> = route_path
                    .split('/')
                    .collect();
                let n_segments = template_segments.len();

                let parse_arm = match &variant.fields {
                    Fields::Named(fields) => {
                        let field_names: Vec<_> = fields
                            .named
                            .iter()
                            .map(|f| &f.ident)
                            .collect();

                        let static_checks: Vec<_> = template_segments
                            .iter()
                            .enumerate()
                            .filter(|(_, s)| !s.starts_with('{') || !s.ends_with('}'))
                            .map(|(i, s)| quote! { parts[#i] == #s })
                            .collect();

                        let field_extracts = fields.named.iter().map(|f| {
                            let fname = f.ident.as_ref().unwrap();
                            let placeholder = format!("{{{}}}", fname);
                            if let Some(idx) = template_segments.iter().position(|s| *s == placeholder) {
                                quote! { let #fname = parts[#idx].parse().ok()?; }
                            } else if let Some((key, _)) = route_qpairs.iter().find(|(_, v)| *v == placeholder) {
                                quote! { let #fname = __query.get(#key).and_then(|v| v.parse().ok())?; }
                            } else {
                                quote! { let #fname = __query.get(stringify!(#fname)).and_then(|v| v.parse().ok())?; }
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
                    }
                    Fields::Unnamed(fields) => {
                        let n_fields = fields.unnamed.len();
                        let placeholder_indices: Vec<usize> = template_segments
                            .iter()
                            .enumerate()
                            .filter(|(_, s)| **s == "{}")
                            .map(|(i, _)| i)
                            .collect();

                        let static_checks: Vec<_> = template_segments
                            .iter()
                            .enumerate()
                            .filter(|(_, s)| **s != "{}")
                            .map(|(i, s)| quote! { parts[#i] == #s })
                            .collect();

                        let arg_names: Vec<_> = (0..n_fields)
                            .map(|i| format_ident!("arg_{}", i))
                            .collect();
                        let field_extracts = arg_names
                            .iter()
                            .zip(placeholder_indices.iter())
                            .map(|(name, idx)| {
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
                    }
                    Fields::Unit => {
                        quote! {
                            if path == #route_path {
                                return Some(Self::#variant_name);
                            }
                        }
                    }
                };

                parse_arms.push(parse_arm);
            }
        }

        // Only override the trait's default `redirect` when at least one variant
        // declared a `#[redirect(...)]`; otherwise fall back to the default (no-op).
        let redirect_method = if redirect_arms.is_empty() {
            quote! {}
        } else {
            quote! {
                fn redirect(&self, ctx: &aimer::widget::base::BuildContext) -> Option<Self> {
                    let _ = ctx;
                    match self {
                        #(#redirect_arms)*
                        #[allow(unreachable_patterns)]
                        _ => None,
                    }
                }
            }
        };

        quote! {
            #item_enum

            impl router::Route for #enum_name {
                fn parse(full_path: &str) -> Option<#enum_name> {
                    let (path, __query) = router::split_path_query(full_path);
                    let _ = &__query;
                    #(#parse_arms)*
                    None
                }

                fn format(&self) -> String {
                    match self {
                        #(#format_arms)*
                        #[allow(unreachable_patterns)]
                        _ => "/".to_string()
                    }
                }

                fn name(&self) -> Option<&'static str> {
                    match self {
                        #(#name_arms)*
                        #[allow(unreachable_patterns)]
                        _ => None,
                    }
                }

                fn resolve_named(name: &str, params: &std::collections::HashMap<String, String>) -> Option<Self> {
                    let _ = params;
                    match name {
                        #(#resolve_arms)*
                        _ => {}
                    }
                    None
                }

                #redirect_method
            }

            impl aimer::widget::Widget for #enum_name {
                fn to_element(&self, ctx: &aimer::widget::base::BuildContext) -> aimer::widget::AnyElement {
                    router::Router::build(self, ctx).to_element(ctx)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn generated_widget_uses_any_element_owner() {
        let generated = RouterCodegen::generate(quote! {
            enum TestRouter {
                #[route("/")]
                Home,
            }
        })
        .to_string();

        assert!(generated.contains("-> aimer :: widget :: AnyElement"));
        assert!(!generated.contains("Box < dyn aimer :: widget :: Element >"));
    }
}
