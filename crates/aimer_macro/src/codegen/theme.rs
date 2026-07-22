use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_quote};

pub(crate) fn style_path() -> syn::Result<TokenStream> {
    if let Ok(found) = crate_name("aimer") {
        return Ok(match found {
            FoundCrate::Itself => quote!(crate::style),
            FoundCrate::Name(name) => {
                let name = Ident::new(&name, Span::call_site());
                quote!(::#name::style)
            }
        });
    }

    match crate_name("aimer_style") {
        Ok(FoundCrate::Itself) => Ok(quote!(crate)),
        Ok(FoundCrate::Name(name)) => {
            let name = Ident::new(&name, Span::call_site());
            Ok(quote!(::#name))
        }
        Err(error) => Err(syn::Error::new(
            Span::call_site(),
            format!("Theme derive requires a dependency on `aimer` or `aimer_style`: {error}"),
        )),
    }
}

pub(crate) fn generate_theme_impl(
    input: DeriveInput,
    style_path: TokenStream,
) -> syn::Result<TokenStream> {
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            Fields::Unnamed(_) | Fields::Unit => {
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    "Theme can only be derived for structs with named fields",
                ));
            }
        },
        Data::Enum(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "Theme cannot be derived for enums",
            ));
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "Theme cannot be derived for unions",
            ));
        }
    };

    let name = &input.ident;
    let mut animation_generics = input.generics.clone();
    for field in fields {
        let ty = &field.ty;
        animation_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(
                #ty: #style_path::__private::Animatable + ::core::clone::Clone
            ));
    }

    let mut theme_generics = animation_generics.clone();
    let (_, input_ty_generics, _) = input
        .generics
        .split_for_impl();
    theme_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(
            #name #input_ty_generics: ::core::clone::Clone + ::core::cmp::PartialEq + 'static
        ));

    let (animation_impl_generics, animation_ty_generics, animation_where_clause) =
        animation_generics.split_for_impl();
    let (theme_impl_generics, theme_ty_generics, theme_where_clause) =
        theme_generics.split_for_impl();
    let interpolated_fields = fields.iter().map(|field| {
        let ident = field
            .ident
            .as_ref()
            .expect("named fields have identifiers");
        quote! {
            #ident: #style_path::__private::Animatable::lerp(&self.#ident, &other.#ident, t)
        }
    });

    Ok(quote! {
        impl #animation_impl_generics #style_path::__private::Animatable
            for #name #animation_ty_generics #animation_where_clause
        {
            fn lerp(&self, other: &Self, t: f32) -> Self {
                if t <= 0.0 {
                    return self.clone();
                }
                if t >= 1.0 {
                    return other.clone();
                }
                Self {
                    #(#interpolated_fields,)*
                }
            }
        }

        impl #theme_impl_generics #style_path::Theme
            for #name #theme_ty_generics #theme_where_clause
        {}
    })
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse_quote;

    use super::*;

    #[test]
    fn named_struct_generates_fieldwise_interpolation_and_theme_impl() {
        let input = parse_quote! {
            struct AppTheme {
                accent: f32,
                radius: i32,
            }
        };

        let output = generate_theme_impl(input, quote!(::aimer::style))
            .expect("named structs should derive Theme")
            .to_string();

        assert!(output.contains("Animatable :: lerp (& self . accent , & other . accent , t)"));
        assert!(output.contains("Animatable :: lerp (& self . radius , & other . radius , t)"));
        assert!(output.contains("impl :: aimer :: style :: Theme for AppTheme"));
        assert!(output.contains("if t <= 0.0"));
        assert!(output.contains("if t >= 1.0"));
    }

    #[test]
    fn generic_struct_preserves_generics_where_clause_and_adds_field_bounds() {
        let input = parse_quote! {
            struct GenericTheme<T>
            where
                T: Send,
            {
                value: T,
            }
        };

        let output = generate_theme_impl(input, quote!(::aimer::style))
            .expect("generic named structs should derive Theme")
            .to_string();

        assert!(output.contains("impl < T >"));
        assert!(output.contains("T : Send"));
        assert!(output.contains("T : :: aimer :: style :: __private :: Animatable"));
        assert!(output.contains(":: core :: clone :: Clone"));
    }

    #[test]
    fn tuple_struct_has_a_targeted_diagnostic() {
        let error = generate_theme_impl(
            parse_quote!(
                struct TupleTheme(f32);
            ),
            quote!(::aimer::style),
        )
        .expect_err("tuple structs must be rejected");

        assert_eq!(
            error.to_string(),
            "Theme can only be derived for structs with named fields"
        );
    }

    #[test]
    fn unit_struct_has_a_targeted_diagnostic() {
        let error = generate_theme_impl(
            parse_quote!(
                struct UnitTheme;
            ),
            quote!(::aimer::style),
        )
        .expect_err("unit structs must be rejected");

        assert_eq!(
            error.to_string(),
            "Theme can only be derived for structs with named fields"
        );
    }

    #[test]
    fn enum_has_a_targeted_diagnostic() {
        let error = generate_theme_impl(
            parse_quote!(
                enum ThemeChoice {
                    Light,
                    Dark,
                }
            ),
            quote!(::aimer::style),
        )
        .expect_err("enums must be rejected");

        assert_eq!(error.to_string(), "Theme cannot be derived for enums");
    }
}
