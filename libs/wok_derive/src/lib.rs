mod derime;
mod param_derive;

#[proc_macro_derive(Param, attributes(param))]
pub fn param_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match param_derive::do_param_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into(),
    }
}

#[proc_macro_derive(Resource, attributes(resource))]
pub fn resource_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match resorce_derive::do_resorce_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into(),
    }
}

mod resorce_derive {
    use quote::quote;

    use crate::{
        CompileError,
        derime::{self, BoolParser, KeyIdent, OptionalAttr},
    };

    pub fn do_resorce_derive(
        ast: syn::DeriveInput,
    ) -> Result<proc_macro2::TokenStream, CompileError> {
        let (mutable, usage) = crate::derime::parse_attrs(
            "resource",
            &ast.attrs,
            (
                OptionalAttr((KeyIdent("mutable"), BoolParser)),
                OptionalAttr((
                    KeyIdent("usage"),
                    derime::IdentValueParser::<crate::param_derive::Usage>::new(),
                )),
            ),
        )?;

        let mutable = mutable.unwrap_or(false);
        let usage = usage.unwrap_or_default();

        let usage_path = match usage {
            crate::param_derive::Usage::Core => quote! { crate::resources },
            crate::param_derive::Usage::Lib => quote! { wok_core::resources },
            crate::param_derive::Usage::Crate => quote! { wok::prelude },
        };

        let mutable_ident = if mutable {
            quote! { Mutable }
        } else {
            quote! { Immutable }
        };

        let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
        let thing_name = &ast.ident;

        let quote = quote! {
            impl #impl_generics #usage_path::Resource for #thing_name #ty_generics #where_clause {
                type Mutability = #usage_path :: #mutable_ident;
            }
        };

        Ok(quote)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn empty() {
            let input: syn::DeriveInput = syn::parse_quote! {
                struct Foo;
            };

            let output = do_resorce_derive(input);
            assert!(output.is_ok(), "{:?}", output.unwrap_err());
            assert_eq!(
                output.unwrap().to_string(),
                quote! {
                    impl wok::prelude::Resource for Foo {
                        type Mutability = wok::prelude::Immutable;
                    }
                }
                .to_string()
            );
        }
    }
}

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

macro_rules! span_compile_error(
    ($span: expr => $msg: expr) => {
        crate::CompileError(quote::quote_spanned! { $span => compile_error!($msg) }.into())
    };

    ($span: expr => $msg: expr, $( $param: expr ),*) => {
        crate::CompileError(quote::quote_spanned! { $span => compile_error!($msg, $( $param ),*) }.into())
    }
);

pub(crate) use span_compile_error;
