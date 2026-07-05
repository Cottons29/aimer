use proc_macro::TokenStream;
use quote::quote;
use syn::{parse::Parse, parse_macro_input, LitStr, Token};

pub struct UniqueKeyInput {
    pub(crate) prefix: Option<LitStr>,
}

impl Parse for UniqueKeyInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            Ok(Self { prefix: None })
        } else {
            Ok(Self {
                prefix: Some(input.parse()?),
            })
        }
    }
}

