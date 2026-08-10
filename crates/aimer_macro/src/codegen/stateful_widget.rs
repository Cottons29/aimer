use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemStruct, parse2};

/// Emits the struct together with the `Widget` impl that hands it to a
/// `StatefulElement`, for the `#[widget(Stateful)]` attribute form.
///
/// The attribute *replaces* the item it is written on, so the item has to be
/// emitted again here. A malformed input is returned unchanged so that the
/// compiler reports the syntax error on the user's own tokens rather than on
/// tokens this macro invented.
pub fn generate_stateful_widget_impl(input: TokenStream) -> TokenStream {
    let item_struct = match parse2::<ItemStruct>(input.clone()) {
        Ok(s) => s,
        Err(_) => return input,
    };
    let widget_impl = stateful_widget_impl(&item_struct);

    quote! {
        #item_struct
        #widget_impl
    }
}

/// Emits only the `Widget` impl, for the `#[derive(StatefulWidget)]` form.
///
/// A derive is expanded *beside* the item it is written on, which the compiler
/// keeps: emitting the struct again would define it twice. There is likewise
/// nothing to fall back to when the input is not a struct, so the error is
/// reported instead of being swallowed.
pub fn derive_stateful_widget(input: TokenStream) -> TokenStream {
    match parse2::<ItemStruct>(input) {
        Ok(item_struct) => stateful_widget_impl(&item_struct),
        Err(err) => err.to_compile_error(),
    }
}

/// The `Widget` impl both forms share.
fn stateful_widget_impl(item_struct: &ItemStruct) -> TokenStream {
    let struct_name = &item_struct.ident;
    let (impl_generics, ty_generics, where_clause) = item_struct.generics.split_for_impl();

    // Detect if the struct has a `key` field
    let has_key = item_struct
        .fields
        .iter()
        .any(|f| f.ident.as_ref().is_some_and(|i| i == "key"));

    let key_pass = if has_key {
        quote! { self.key.clone() }
    } else {
        quote! { None }
    };

    let key_method = if has_key {
        quote! {
            fn key(&self) -> Option<aimer::widget::key::Key> {
                self.key.clone()
            }
        }
    } else {
        quote! {}
    };

    quote! {
        impl #impl_generics aimer::widget::Widget for #struct_name #ty_generics #where_clause {
            #key_method

            fn to_element(&self, ctx: &widget::base::BuildContext) -> aimer::widget::AnyElement {
                aimer::widget::StatefulElement::from_widget(self, ctx, stringify!(#struct_name), #key_pass)
            }
            fn debug_name(&self) -> &'static str {
                stringify!(#struct_name)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;

    #[test]
    fn generated_widget_uses_recoverable_stateful_conversion() {
        let output = generate_stateful_widget_impl(quote! {
            struct PanicBoundaryWidget;
        })
        .to_string();

        assert!(output.contains("StatefulElement :: from_widget"));
        assert!(output.contains("aimer :: widget :: AnyElement"));
        assert!(!output.contains("StatefulElement :: new_with_name"));
        assert!(!output.contains("Box < dyn widget :: Element >"));
    }

    #[test]
    fn the_derive_leaves_the_struct_to_the_compiler() {
        let output = derive_stateful_widget(quote! {
            #[derive(StatefulWidget)]
            struct CounterWidget {
                initial_count: i32,
            }
        })
        .to_string();

        assert!(!output.contains("struct CounterWidget"));
        assert!(output.contains("impl aimer :: widget :: Widget for CounterWidget"));
    }

    #[test]
    fn the_derive_and_the_attribute_wire_the_widget_up_the_same_way() {
        let item = quote! {
            struct CounterWidget {
                key: Option<Key>,
            }
        };

        let attribute = generate_stateful_widget_impl(item.clone()).to_string();
        let derived = derive_stateful_widget(item).to_string();

        let (_, attribute_impl) = attribute
            .split_once("impl aimer :: widget :: Widget")
            .expect("the attribute form implements Widget");
        let (_, derived_impl) = derived
            .split_once("impl aimer :: widget :: Widget")
            .expect("the derive implements Widget");
        assert_eq!(attribute_impl, derived_impl);
    }

    #[test]
    fn the_derive_rejects_what_is_not_a_struct() {
        let output = derive_stateful_widget(quote! {
            enum NotAWidget {
                Nope,
            }
        })
        .to_string();

        assert!(output.contains("compile_error"));
    }
}
