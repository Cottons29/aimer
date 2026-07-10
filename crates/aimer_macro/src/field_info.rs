use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::spanned::Spanned;
use syn::{Error, Expr, Field, Meta, Type};

pub struct FieldInfo<'a> {
    pub ident: &'a Ident,
    pub ty: &'a Type,
    pub skip: bool,
    pub default: Option<TokenStream>,
    pub into: bool,
    pub first: bool,
    pub dyn_iter: bool,
    pub async_wrapper: Option<String>,
    pub docs: Vec<String>,
}

impl<'a> FieldInfo<'a> {
    pub(crate) fn parse_field_info(field: &'a Field) -> Result<Self, Error> {
        let ident = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new(field.span(), "Constructor can only be derived for structs with named fields"))?;
        let ty = &field.ty;
        let mut skip = false;
        let mut default = None;
        let mut into = false;
        let mut first = false;
        let mut dyn_iter = false;
        let mut async_wrapper: Option<String> = None;
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
                    } else if meta.path.is_ident("async_wrapper") {
                        let value: syn::LitStr = meta.value()?.parse()?;
                        async_wrapper = Some(value.value());
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

        #[allow(clippy::collapsible_if)]
        if default.is_none() {
            if crate::codegen::auto_wrapper::AutoWrapper::new(ty).is_option() {
                default = Some(quote! { None });
            }
        }

        Ok(Self { ident, ty, skip, default, into, first, dyn_iter, async_wrapper, docs })
    }
}
