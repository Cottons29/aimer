pub(crate) mod auto_wrapper;
mod raw_widget;
pub mod router;
mod stateful_widget;
mod stateless_widget;

use proc_macro2::TokenStream;

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
