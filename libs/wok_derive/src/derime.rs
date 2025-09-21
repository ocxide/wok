use std::str::FromStr;

use proc_macro2::Span;
use syn::{
    Expr, Ident, Lit, MetaNameValue, Token, parse::Parser, punctuated::Punctuated, spanned::Spanned,
};

use crate::{CompileError, span_compile_error};

fn key_value(
    attr: &syn::Attribute,
    namespace: &str,
) -> Result<Punctuated<MetaNameValue, Token![,]>, CompileError> {
    let tokens = match &attr.meta {
        syn::Meta::List(meta_list) => &meta_list.tokens,
        _ => return Err(span_compile_error!(attr.span() => "Expected #[{}(...)]", namespace)),
    };

    let keyvalues = Punctuated::<MetaNameValue, Token![,]>::parse_terminated
        .parse2(tokens.clone())
        .map_err(|_| span_compile_error!(attr.span() => "Invalid attribute syntax"))?;

    if keyvalues.is_empty() {
        return Err(
            span_compile_error!(attr.span() => "Expected at least one key-value pair in #[{}(...)]", namespace),
        );
    }

    Ok(keyvalues)
}

pub trait AttrsMatch<Marker> {
    type Out;

    fn attrs_match(
        self,
        span: Option<Span>,
        key_values: impl Iterator<Item = MetaNameValue>,
    ) -> Result<Self::Out, CompileError>;
}

pub struct RequiredParsing;
impl<Marker1, K1: PathMatch + ReprValueRef, V1: ValueParser<Marker1>>
    AttrsMatch<(Marker1, RequiredParsing)> for (K1, V1)
{
    type Out = V1::Out;
    fn attrs_match(
        self,
        span: Option<Span>,
        mut key_values: impl Iterator<Item = MetaNameValue>,
    ) -> Result<Self::Out, CompileError> {
        let Some(span) = span else {
            return Err(
                span_compile_error!(Span::call_site() => "Expected at least one #[{}()] attribute", namespace),
            );
        };

        let Some(kv) = key_values.find(|kv| self.0.path_match(&kv.path)) else {
            return Err(
                span_compile_error!(span => "Expected at least one single {} = ...", K1::repr()),
            );
        };

        if key_values.next().is_some() {
            return Err(
                span_compile_error!(span => "Expected at most one single {} = {}", K1::repr(), V1::repr()),
            );
        };

        self.1.parse(kv.value)
    }
}

pub struct OptionalParsing;
impl<Marker1, K1: PathMatch + ReprValueRef, V1: ValueParser<Marker1>>
    AttrsMatch<(Marker1, OptionalParsing)> for OptionalAttr<(K1, V1)>
{
    type Out = Option<V1::Out>;
    fn attrs_match(
        self,
        span: Option<Span>,
        mut key_values: impl Iterator<Item = MetaNameValue>,
    ) -> Result<Self::Out, CompileError> {
        let span = match span {
            Some(span) => span,
            None => return Ok(None),
        };

        let Some(kv) = key_values.find(|kv| self.0.0.path_match(&kv.path)) else {
            return Err(span_compile_error!(span => "Expected a {} = ...", K1::repr()));
        };

        if key_values.next().is_some() {
            return Err(
                span_compile_error!(span => "Expected at most one single {} = {}", K1::repr(), V1::repr()),
            );
        };

        self.0.1.parse(kv.value).map(Some)
    }
}

pub struct KeyIdent<'s>(pub &'s str);

impl ReprValueRef for KeyIdent<'_> {
    fn repr(&self) -> &str {
        self.0
    }
}

trait ReprValueRef {
    fn repr(&self) -> &str;
}

pub trait PathMatch {
    fn path_match(&self, path: &syn::Path) -> bool;
}

impl PathMatch for KeyIdent<'_> {
    fn path_match(&self, path: &syn::Path) -> bool {
        path.is_ident(self.0)
    }
}

pub trait ValueParser<Marker> {
    type Out;
    fn parse(self, expr: Expr) -> Result<Self::Out, CompileError>;
}

pub struct OptionalAttr<V>(pub V);

pub struct IdentValue<T>(std::marker::PhantomData<T>);

impl<T> IdentValue<T> {
    pub const fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T> Default for IdentValue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M, I: IdentValueParse<M>> ValueParser<M> for IdentValue<I> {
    type Out = I;

    fn parse(self, expr: Expr) -> Result<Self::Out, CompileError> {
        let span = expr.span();
        let segments = match expr {
            Expr::Path(syn::ExprPath { path, .. }) => path.segments,
            _ => return Err(span_compile_error!(expr.span() => "Expected an identifier")),
        };

        let ident = segments
            .into_iter()
            .next()
            .map(|s| s.ident)
            .ok_or_else(|| span_compile_error!(span => "Expected an identifier"))?;

        I::parse(ident)
    }
}

pub trait ReprValue {
    fn repr() -> &'static str;
}

pub trait IdentValueParse<Marker>: Sized {
    fn repr() -> &'static str;
    fn parse(ident: Ident) -> Result<Self, CompileError>;
}

pub struct IdentFromStr;
impl<S: FromStr + ReprValue> IdentValueParse<IdentFromStr> for S {
    fn repr() -> &'static str {
        S::repr()
    }

    fn parse(ident: Ident) -> Result<Self, CompileError> {
        S::from_str(&ident.to_string()).map_err(|e| span_compile_error!(ident.span() => "{}", e))
    }
}

pub struct BoolParser;

impl ValueParser<bool> for BoolParser {
    type Out = bool;

    fn parse(self, expr: Expr) -> Result<Self::Out, CompileError> {
        match expr {
            Expr::Lit(syn::ExprLit {
                lit: Lit::Bool(lit),
                ..
            }) => Ok(lit.value),
            _ => Err(span_compile_error!(expr.span() => "Expected a boolean literal")),
        }
    }
}

pub fn parse_attrs<Marker, P: AttrsMatch<Marker>>(
    namespace: &str,
    attrs: &[syn::Attribute],
    parser: P,
) -> Result<P::Out, CompileError> {
    let span = match attrs {
        [] => None,
        [attr] => Some(attr.span()),
        [attr1, .., attr2] => Some(attr1.span().join(attr2.span()).unwrap()),
    };

    let entries: Vec<_> = attrs
        .iter()
        .map(|attr| key_value(attr, namespace))
        .collect::<Result<_, _>>()?;

    let key_values = entries.into_iter().flatten();
    parser.attrs_match(span, key_values)
}
