use proc_macro2::TokenStream;
use quote::quote;
use syn::{Type, TypePath, GenericArgument, PathArguments, TypeTraitObject, TraitBound, TraitBoundModifier};

#[allow(clippy::large_enum_variant)]
pub enum AutoWrapper {
    Box(Box<AutoWrapper>),
    Arc(Box<AutoWrapper>),
    Rc(Box<AutoWrapper>),
    Option(Box<AutoWrapper>),
    RefCell(Box<AutoWrapper>),
    UnsafeCell(Box<AutoWrapper>),
    Vec(Box<AutoWrapper>),
    Terminal(Type),
}

impl AutoWrapper {
    pub fn new(ty: &Type) -> Self {
        if let Some(inner) = get_option_inner(ty) {
            return Self::Option(Box::new(Self::new(inner)));
        }

        if let Some(inner) = get_type_inner(ty, "Box") {
            return Self::Box(Box::new(Self::new(inner)));
            return Self::Box(Box::new(Self::new(inner)));
        }

        if let Some(inner) = get_type_inner(ty, "Arc") {
            return Self::Arc(Box::new(Self::new(inner)));
        }

        if let Some(inner) = get_type_inner(ty, "Rc") {
            return Self::Rc(Box::new(Self::new(inner)));
        }

        if let Some(inner) = get_type_inner(ty, "RefCell") {
            return Self::RefCell(Box::new(Self::new(inner)));
        }

        if let Some(inner) = get_type_inner(ty, "UnsafeCell") {
            return Self::UnsafeCell(Box::new(Self::new(inner)));
        }

        if let Some(inner) = get_collection_inner(ty) {
            return Self::Vec(Box::new(Self::new(inner)));
        }

        AutoWrapper::Terminal(ty.clone())
    }

    pub fn get_type(&self) -> TokenStream {
        match self {
            AutoWrapper::Box(inner) => {
                let inner_ty = inner.get_type();
                quote! { Box<#inner_ty> }
            }
            AutoWrapper::Arc(inner) => {
                let inner_ty = inner.get_type();
                quote! { std::sync::Arc<#inner_ty> }
            }
            AutoWrapper::Rc(inner) => {
                let inner_ty = inner.get_type();
                quote! { std::rc::Rc<#inner_ty> }
            }
            AutoWrapper::Option(inner) => {
                let inner_ty = inner.get_type();
                quote! { Option<#inner_ty> }
            }
            AutoWrapper::RefCell(inner) => {
                let inner_ty = inner.get_type();
                quote! { std::cell::RefCell<#inner_ty> }
            }
            AutoWrapper::UnsafeCell(inner) => {
                let inner_ty = inner.get_type();
                quote! { std::cell::UnsafeCell<#inner_ty> }
            }
            AutoWrapper::Vec(inner) => {
                let inner_ty = inner.get_type();
                quote! { Vec<#inner_ty> }
            }
            AutoWrapper::Terminal(ty) => quote! { #ty },
        }
    }

    pub fn wrap_expr(&self, expr: TokenStream) -> TokenStream {
        match self {
            AutoWrapper::Box(inner) => {
                let inner_expr = inner.wrap_expr(expr);
                quote! { Box::new(#inner_expr) }
            }
            AutoWrapper::Arc(inner) => {
                let inner_expr = inner.wrap_expr(expr);
                quote! { std::sync::Arc::new(#inner_expr) }
            }
            AutoWrapper::Rc(inner) => {
                let inner_expr = inner.wrap_expr(expr);
                quote! { std::rc::Rc::new(#inner_expr) }
            }
            AutoWrapper::UnsafeCell(inner) => {
                let inner_expr = inner.wrap_expr(expr);
                quote! { std::cell::UnsafeCell::new(#inner_expr) }
            }
            AutoWrapper::Option(inner) => {
                let inner_expr = inner.wrap_expr(expr);
                quote! { Some(#inner_expr) }
            }
            AutoWrapper::RefCell(inner) => {
                let inner_expr = inner.wrap_expr(expr);
                quote! { std::cell::RefCell::new(#inner_expr) }
            }
            AutoWrapper::Vec(inner) => {
                // Vec is a bit special in how it was handled before (BoxCollection)
                // If it's the outermost, it usually expects an iterator or array in the macro.
                // But if we are wrapping a single element into a Vec? 
                // Previous BoxCollection was specifically Vec<Box<dyn Widget>>.
                let inner_expr = inner.wrap_expr(expr);
                quote! { vec![#inner_expr] }
            }
            AutoWrapper::Terminal(_) => expr,
        }
    }

    /// Returns true if this type is `Box<dyn Widget>` or a generic bound with `Widget + 'static`,
    /// meaning items of this type should NOT be wrapped in `Box::new(...)` in the array rule.
    pub fn is_widget_boxed(ty: &Type) -> bool {
        // Check for Box<dyn Widget> or Box<dyn Widget + 'static>
        if let Some(inner) = get_type_inner(ty, "Box") {
            if let Type::TraitObject(TypeTraitObject { bounds, .. }) = inner {
                return bounds.iter().any(|b| {
                    if let syn::TypeParamBound::Trait(TraitBound { path, modifier, .. }) = b {
                        if matches!(modifier, TraitBoundModifier::None) {
                            if let Some(seg) = path.segments.last() {
                                return seg.ident == "Widget";
                            }
                        }
                    }
                    false
                });
            }
        }
        // Check for generic type param bound like `T: Widget + 'static`
        if let Type::Path(TypePath { path, .. }) = ty {
            if path.segments.len() == 1 {
                // Single-segment path — likely a generic type param, treat as widget-boxed
                // if the field is annotated with dyn_iter (handled at call site)
            }
        }
        false
    }

    /// Returns true if the type is a single-segment path (i.e., a generic type param like `T`),
    /// which indicates `Vec<T>` where `T: Widget + 'static` — items should not be wrapped in `Box::new`.
    pub fn is_generic_widget_param(ty: &Type) -> bool {
        if let Type::Path(TypePath { qself: None, path }) = ty {
            path.leading_colon.is_none() && path.segments.len() == 1
        } else {
            false
        }
    }

    pub fn is_option(&self) -> bool {
        match self {
            AutoWrapper::Option(_) => true,
            AutoWrapper::Box(inner)
            | AutoWrapper::UnsafeCell(inner)
            | AutoWrapper::Arc(inner)
            | AutoWrapper::Rc(inner)
            | AutoWrapper::RefCell(inner)
            | AutoWrapper::Vec(inner) => inner.is_option(),
            AutoWrapper::Terminal(_) => false,
        }
    }
}

#[allow(clippy::collapsible_if)]
fn get_option_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Option" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}
#[allow(clippy::collapsible_if)]
fn get_box_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Box" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}



fn get_type_inner<'a>(ty: &'a Type, name: &str) -> Option<&'a Type> {
    #[allow(clippy::collapsible_if)]
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == name {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}
#[allow(clippy::collapsible_if)]
fn get_arc_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Arc" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}
#[allow(clippy::collapsible_if)]
fn get_rc_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Rc" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}
#[allow(clippy::collapsible_if)]
fn get_refcell_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "RefCell" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}

#[allow(clippy::collapsible_if)]
fn get_collection_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident == "Vec" || segment.ident == "Array" {
                if let PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}