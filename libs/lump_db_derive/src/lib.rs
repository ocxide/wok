use as_surreal_bind::do_as_surreal_bind_derive;

#[proc_macro_derive(AsSurrealBind, attributes(surreal_bind))]
pub fn as_surreal_bind_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match do_as_surreal_bind_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into(),
    }
}

#[proc_macro_derive(FromSurrealBind, attributes(surreal_bind))]
pub fn from_surreal_bind_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match from_surreal_bind::from_surreal_db_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into(),
    }
}

mod as_surreal_bind;
mod from_surreal_bind;

#[derive(Debug)]
struct CompileError(proc_macro2::TokenStream);

impl From<CompileError> for proc_macro::TokenStream {
    fn from(err: CompileError) -> Self {
        err.0.into()
    }
}

impl From<CompileError> for proc_macro2::TokenStream {
    fn from(err: CompileError) -> Self {
        err.0
    }
}

macro_rules! span_compile_error(($span: expr => $msg: expr) => {
    crate::CompileError(quote::quote_spanned! { $span => compile_error!($msg) }.into())
    });

pub(crate) use span_compile_error;

mod util {
    use syn::{MetaNameValue, Token, parse::Parser, punctuated::Punctuated, spanned::Spanned};

    use crate::CompileError;

    pub enum InvalidAttrReason {
        NotScoped,
        Other(CompileError),
    }

    pub fn named_attrs(
        attrs: &[syn::Attribute],
    ) -> Result<Option<MetaNameValue>, InvalidAttrReason> {
        let Some(attr) = attrs.first() else {
            return Ok(None);
        };

        let tokens = match &attr.meta {
            syn::Meta::List(meta_list) => &meta_list.tokens,
            _ => return Err(InvalidAttrReason::NotScoped),
        };

        dbg!(tokens.to_string());
        let keyvalues = Punctuated::<MetaNameValue, Token![,]>::parse_terminated
            .parse2(tokens.clone())
            .map_err(|_| {
                InvalidAttrReason::Other(
                    span_compile_error!(attr.span() => "Invalid attribute syntax"),
                )
            })?;

        match keyvalues.first() {
            Some(meta) => Ok(Some(meta.clone())),
            None => Ok(None),
        }
    }
}
