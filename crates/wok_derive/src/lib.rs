mod param_derive;

#[proc_macro_derive(Param, attributes(param))]
pub fn param_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match param_derive::do_param_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => proc_macro2::TokenStream::from(err).into(),
    }
}

#[proc_macro_derive(Resource, attributes(resource))]
pub fn resource_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match resorce_derive::do_resorce_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => proc_macro2::TokenStream::from(err).into(),
    }
}

mod resorce_derive {
    use quote::quote;

    use derime::{BoolParser, CompileError, KeyIdent, OptionalAttr};

    pub fn do_resorce_derive(
        ast: syn::DeriveInput,
    ) -> Result<proc_macro2::TokenStream, CompileError> {
        let (mutable, usage) = derime::parse_attrs(
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
                enum Foo {}
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

        #[test]
        fn ignores_others() {
            let input: syn::DeriveInput = syn::parse_quote! {
                #[derive(serde::Deserialize)]
                #[serde(untagged)]
                enum Foo {}
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
