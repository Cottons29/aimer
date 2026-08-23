use quote::ToTokens;
use sha2::{Digest, Sha256};
use syn::visit_mut::{self, VisitMut};

use super::{ShadowError, ShadowErrorKind};

/// Deterministic SHA-256 identity for one syntax expression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AstFingerprint(String);

impl AstFingerprint {
    /// Returns the lowercase hexadecimal digest.
    #[inline]
    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for AstFingerprint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Fingerprints a parsed expression, optionally using a static portable key.
pub fn fingerprint_expression(
    expression: &syn::Expr,
    portable_key: Option<&syn::Expr>,
) -> Result<AstFingerprint, ShadowError> {
    let canonical = if let Some(key) = portable_key {
        if !is_portable_key(key) {
            return Err(ShadowError::new(
                ShadowErrorKind::DynamicFlow,
                "portable key must be a literal, static path, or a static aggregate",
            ));
        }
        format!("portable:{}", key.to_token_stream())
    } else {
        let mut normalized = expression.clone();
        ClosureLiteralNormalizer::default().visit_expr_mut(&mut normalized);
        format!("ast:{}", normalized.to_token_stream())
    };
    let digest = Sha256::digest(canonical.as_bytes());
    Ok(AstFingerprint(hex::encode(digest)))
}

/// Parses and fingerprints an expression and optional portable key.
pub fn fingerprint_source(
    expression: &str,
    portable_key: Option<&str>,
) -> Result<AstFingerprint, ShadowError> {
    let expression = syn::parse_str(expression).map_err(|error| {
        ShadowError::new(ShadowErrorKind::MalformedSource, error.to_string())
    })?;
    let key = portable_key
        .map(syn::parse_str)
        .transpose()
        .map_err(|error| ShadowError::new(ShadowErrorKind::MalformedSource, error.to_string()))?;
    fingerprint_expression(&expression, key.as_ref())
}

fn is_portable_key(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Array(array) => array.elems.iter().all(is_portable_key),
        syn::Expr::Group(group) => is_portable_key(&group.expr),
        syn::Expr::Lit(_) => true,
        syn::Expr::Paren(paren) => is_portable_key(&paren.expr),
        syn::Expr::Path(path) => path.qself.is_none()
            && path.path.segments.iter().all(|segment| matches!(segment.arguments, syn::PathArguments::None))
            && path.path.segments.last().is_some_and(|segment| {
                let name = segment.ident.to_string();
                name.chars().any(|character| character.is_ascii_uppercase())
                    && name.chars().all(|character| !character.is_ascii_lowercase())
            }),
        syn::Expr::Reference(reference) => is_portable_key(&reference.expr),
        syn::Expr::Tuple(tuple) => tuple.elems.iter().all(is_portable_key),
        syn::Expr::Unary(unary) => is_portable_key(&unary.expr),
        _ => false,
    }
}

#[derive(Default)]
struct ClosureLiteralNormalizer {
    closure_depth: usize,
}

impl VisitMut for ClosureLiteralNormalizer {
    fn visit_expr_closure_mut(&mut self, expression: &mut syn::ExprClosure) {
        self.closure_depth += 1;
        visit_mut::visit_expr_closure_mut(self, expression);
        self.closure_depth -= 1;
    }

    fn visit_expr_lit_mut(&mut self, expression: &mut syn::ExprLit) {
        if self.closure_depth != 0 {
            expression.lit = normalized_literal(&expression.lit);
        }
    }

    fn visit_macro_mut(&mut self, expression: &mut syn::Macro) {
        if self.closure_depth != 0 {
            let normalized = normalize_token_literals(&expression.tokens.to_string());
            if let Ok(tokens) = normalized.parse() {
                expression.tokens = tokens;
            }
        }
    }
}

fn normalized_literal(literal: &syn::Lit) -> syn::Lit {
    let span = literal.span();
    match literal {
        syn::Lit::Str(_) => syn::Lit::Str(syn::LitStr::new("", span)),
        syn::Lit::ByteStr(_) => syn::Lit::ByteStr(syn::LitByteStr::new(&[], span)),
        syn::Lit::Byte(_) => syn::Lit::Byte(syn::LitByte::new(0, span)),
        syn::Lit::Char(_) => syn::Lit::Char(syn::LitChar::new('\0', span)),
        syn::Lit::Int(value) => {
            syn::Lit::Int(syn::LitInt::new(&format!("0{}", value.suffix()), span))
        }
        syn::Lit::Float(value) => {
            syn::Lit::Float(syn::LitFloat::new(&format!("0.0{}", value.suffix()), span))
        }
        syn::Lit::Bool(_) => syn::Lit::Bool(syn::LitBool::new(false, span)),
        _ => literal.clone(),
    }
}

fn normalize_token_literals(tokens: &str) -> String {
    let mut output = String::with_capacity(tokens.len());
    let mut characters = tokens.char_indices().peekable();
    while let Some((_, character)) = characters.next() {
        if character == '"' {
            output.push_str("\"_\"");
            let mut escaped = false;
            for (_, current) in characters.by_ref() {
                if current == '"' && !escaped {
                    break;
                }
                escaped = current == '\\' && !escaped;
                if current != '\\' {
                    escaped = false;
                }
            }
        } else if character.is_ascii_digit() {
            output.push('0');
            while characters.peek().is_some_and(|(_, current)| {
                current.is_ascii_alphanumeric() || matches!(current, '_' | '.')
            }) {
                characters.next();
            }
        } else {
            output.push(character);
        }
    }
    output
}