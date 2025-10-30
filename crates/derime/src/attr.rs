use std::{fmt::Display, str::FromStr};

use proc_macro2::Span;
use syn::{
    Expr, Ident, Lit, MetaNameValue, Token, parse::Parser, punctuated::Punctuated, spanned::Spanned,
};

use crate::CompileError;

pub struct ScopedAttr<'a> {
    pub scope: &'a Ident,
    pub attr: &'a syn::Attribute,
    pub tokens: &'a proc_macro2::TokenStream,
}

impl<'a> ScopedAttr<'a> {
    pub fn parse(attr: &'a syn::Attribute) -> Result<Self, ParseScopedAttrError> {
        let meta_list = match &attr.meta {
            syn::Meta::List(meta_list) => meta_list,
            _ => return Err(ParseScopedAttrError::NotScoped),
        };

        let scope = &meta_list
            .path
            .segments
            .first()
            .ok_or(ParseScopedAttrError::ScopeIdentInvalid)?
            .ident;

        Ok(ScopedAttr {
            scope,
            attr,
            tokens: &meta_list.tokens,
        })
    }
}

pub enum ParseScopedAttrError {
    NotScoped,
    ScopeIdentInvalid,
}

fn key_value(
    attr: &syn::Attribute,
    namespace: &str,
) -> Result<Option<Punctuated<MetaNameValue, Token![,]>>, CompileError> {
    let span = attr.span();
    let attr = ScopedAttr::parse(attr)
        .map_err(|_| span_compile_error!(span => "Expected #[{}(...)]", namespace))?;


    if attr.scope != namespace {
        return Ok(None);
    }

    let keyvalues = Punctuated::<MetaNameValue, Token![,]>::parse_terminated
        .parse2(attr.tokens.clone())
        .map_err(|_| span_compile_error!(span => "Invalid attribute syntax"))?;

    if keyvalues.is_empty() {
        return Err(
            span_compile_error!(span => "Expected at least one key-value pair in #[{}(...)]", namespace),
        );
    }

    Ok(Some(keyvalues))
}

pub trait AttrsMatch<Marker> {
    type Out;

    fn attrs_match(
        self,
        span: Option<Span>,
        key_values: impl Iterator<Item = MetaNameValue>,
        namespace: &str,
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
        namespace: &str,
    ) -> Result<Self::Out, CompileError> {
        let Some(span) = span else {
            return Err(
                span_compile_error!(Span::call_site() => "Expected at least one #[{}()] attribute", namespace),
            );
        };

        let Some(kv) = key_values.find(|kv| self.0.path_match(&kv.path)) else {
            return Err(
                span_compile_error!(span => "Expected at least one single {} = ...", K1::repr(&self.0)),
            );
        };

        if key_values.next().is_some() {
            return Err(
                span_compile_error!(span => "Expected at most one single {} = {}", K1::repr(&self.0), V1::repr()),
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
        _namespace: &str,
    ) -> Result<Self::Out, CompileError> {
        let span = match span {
            Some(span) => span,
            None => return Ok(None),
        };

        let Some(kv) = key_values.find(|kv| self.0.0.path_match(&kv.path)) else {
            return Ok(None);
        };

        if key_values.next().is_some() {
            let k1 = K1::repr(&self.0.0);
            let v1 = V1::repr();
            return Err(span_compile_error!(span => "Expected at most one single {} = {}", k1, v1));
        };

        self.0.1.parse(kv.value).map(Some)
    }
}

macro_rules! impl_optionals {
($(($marker:ident, $k:ident, $v:ident));*) => {
    impl<
        $( $marker, $k, $v ),*
    > AttrsMatch<(($( $marker ),*), OptionalParsing)> for ( $(OptionalAttr<($k, $v)>,)* )
    where
        $( $k: PathMatch + ReprValueRef, $v: ValueParser<$marker> ),*
    {
        type Out = ( $(Option<$v::Out>),* );

        fn attrs_match(
            self,
            span: Option<Span>,
            mut key_values: impl Iterator<Item = MetaNameValue>,
            _namespace: &str
        ) -> Result<Self::Out, CompileError> {

            #[allow(non_snake_case)]
            let ( $($k),* ) = self;

            $(
            #[allow(non_snake_case)]
            let mut $v = None;
            )*

            let span = match span {
                Some(span) => span,
                None => return Ok(( $( $v ),* )),
            };

            for kv in &mut key_values {
                $(
                    if $k.0.0.path_match(&kv.path) {
                        $v = Some($k.0.1.parse(kv.value)?);
                    } else
                )* {}
            }

            if key_values.next().is_some() {
                return Err(
                    span_compile_error!(span => "Too many args!"),
                );
            };

            Ok(( $( $v ),* ))

        }
    }
};
}
impl_optionals!( (Marker1, K1, V1); (Marker2, K2, V2) );
impl_optionals!( (Marker1, K1, V1); (Marker2, K2, V2); (Marker3, K3, V3) );
impl_optionals!( (Marker1, K1, V1); (Marker2, K2, V2); (Marker3, K3, V3); (Marker4, K4, V4) );

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
    fn repr() -> &'static str;
    fn parse(&self, expr: Expr) -> Result<Self::Out, CompileError>;
}

pub struct OptionalAttr<V>(pub V);

pub struct IdentValueParser<T>(std::marker::PhantomData<T>);

impl<T> IdentValueParser<T> {
    pub const fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T> Default for IdentValueParser<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M, I: IdentValueParse<M>> ValueParser<M> for IdentValueParser<I> {
    type Out = I;

    fn repr() -> &'static str {
        I::repr()
    }

    fn parse(&self, expr: Expr) -> Result<Self::Out, CompileError> {
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
impl<S: FromStr<Err: Display> + ReprValue> IdentValueParse<IdentFromStr> for S {
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

    fn repr() -> &'static str {
        "bool"
    }

    fn parse(&self, expr: Expr) -> Result<Self::Out, CompileError> {
        match expr {
            Expr::Lit(syn::ExprLit {
                lit: Lit::Bool(lit),
                ..
            }) => Ok(lit.value),
            _ => Err(span_compile_error!(expr.span() => "Expected a boolean literal")),
        }
    }
}

pub struct ExprParser;
impl ValueParser<Expr> for ExprParser {
    type Out = Expr;
    fn repr() -> &'static str {
        "expr"
    }

    fn parse(&self, expr: Expr) -> Result<Self::Out, CompileError> {
        Ok(expr)
    }
}

pub struct StringParser;
impl ValueParser<String> for StringParser {
    type Out = String;
    fn repr() -> &'static str {
        "string"
    }

    fn parse(&self, expr: Expr) -> Result<Self::Out, CompileError> {
        match expr {
            Expr::Lit(syn::ExprLit {
                lit: Lit::Str(lit),
                ..
            }) => Ok(lit.value()),
            _ => Err(span_compile_error!(expr.span() => "Expected a string literal")),
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
        .filter_map(|r| r.transpose())
        .collect::<Result<_, _>>()?;

    let key_values = entries.into_iter().flatten();
    parser.attrs_match(span, key_values, namespace)
}

pub struct AttributesParser {
    pub scope: &'static str,
}

impl AttributesParser {
    pub fn new(scope: &'static str) -> Self {
        Self { scope }
    }

    pub fn parse<Marker, P: AttrsMatch<Marker>>(
        &self,
        attrs: &[syn::Attribute],
        parser: P,
    ) -> Result<P::Out, CompileError> {
        parse_attrs(self.scope, attrs, parser)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn skip_others() {
        let input: syn::DeriveInput = syn::parse_quote! {
            #[serde(myprop = false)]
            struct A {}
        };

        let result = super::AttributesParser::new("test")
            .parse(
                &input.attrs,
                super::OptionalAttr((super::KeyIdent("myprop"), super::BoolParser)),
            )
            .unwrap();

        assert_eq!(result, None);
    }

    #[test]
    fn skip_field_others() {
        let input: syn::DeriveInput = syn::parse_quote! {
            struct A {
                #[serde(myprop = false)]
                a: i32,
            }
        };

        let fields = match input.data {
            syn::Data::Struct(syn::DataStruct {
                fields: syn::Fields::Named(syn::FieldsNamed { named, .. }),
                ..
            }) => named,
            _ => unreachable!(),
        };

        let field_a = fields.into_iter().next().unwrap();

        let result = super::AttributesParser::new("test")
            .parse(
                &field_a.attrs,
                super::OptionalAttr((super::KeyIdent("myprop"), super::BoolParser)),
            )
            .unwrap();

        assert_eq!(result, None);
    }
}
