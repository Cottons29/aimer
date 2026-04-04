mod stateful_widget;
mod stateless_widget;
mod raw_widget;
mod constructor;
mod auto_wrapper;
pub mod router;

use proc_macro2::TokenStream;

pub struct ConstructorCodegen;

impl ConstructorCodegen {
    pub fn generate(input: TokenStream) -> TokenStream {
        constructor::constructor_derive(input, false)
    }

    pub fn generate_boxed(input: TokenStream) -> TokenStream {
        constructor::constructor_derive(input, true)
    }
}

pub struct StatelessWidgetCodegen;

impl StatelessWidgetCodegen {
    pub fn generate(input: TokenStream) -> TokenStream {
        stateless_widget::generate_stateless_widget_impl(input)
    }
}

pub struct StatefulWidgetCodegen;

impl StatefulWidgetCodegen {
    pub fn generate(input: TokenStream) -> TokenStream {
        stateful_widget::generate_stateful_widget_impl(input)
    }
}

pub struct RawWidgetCodegen;

impl RawWidgetCodegen {
    pub fn generate(input: TokenStream) -> TokenStream {
        raw_widget::generate_raw_widget_impl(input)
    }
}
