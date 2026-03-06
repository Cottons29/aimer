mod stateful_widget;
mod stateless_widget;
mod constructor;
mod auto_wrapper;

use proc_macro2::TokenStream;

pub struct ConstructorCodegen;

impl ConstructorCodegen {
    pub fn generate(input: TokenStream) -> TokenStream {
        constructor::constructor_derive(input)
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
