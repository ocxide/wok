use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::{DeriveInput, Expr, Index, MetaNameValue, spanned::Spanned};

use crate::{
    CompileError, span_compile_error,
    util::{InvalidAttrReason, named_attrs},
};

pub fn from_surreal_db_derive(ast: DeriveInput) -> Result<TokenStream, CompileError> {
    let span = ast.span();
    let struct_data = match ast.data {
        syn::Data::Struct(data) => data,
        _ => {
            return Err(
                span_compile_error!(span => "FromSurrealBind can only be derived on structs"),
            );
        }
    };

    let new_name: String;
    let mapping_name = match named_attrs(&ast.attrs) {
        Ok(Some(MetaNameValue {
            path,
            value: Expr::Path(expr),
            ..
        })) if path.is_ident("der") => match expr.path.get_ident() {
            Some(ident) => ident.clone(),
            None => {
                return Err(span_compile_error!(span => "Expected #[surreal_bind(der = ...)]"));
            }
        },
        Ok(Some(_)) => {
            return Err(span_compile_error!(span => "Expected #[surreal_bind(der = ...)]"));
        }
        Ok(None) => {
            new_name = "Surreal".to_owned() + &ast.ident.to_string();
            Ident::new(&new_name, ast.ident.span())
        }
        Err(InvalidAttrReason::NotScoped) => {
            return Err(span_compile_error!(span => "Expected #[surreal_bind(...)]"));
        }
        Err(InvalidAttrReason::Other(err)) => return Err(err),
    };

    let thing_name = &ast.ident;
    let trait_path = quote! { FromSurrealBind };

    let vis = &ast.vis;

    let (og_impl_generics, og_ty_generics, og_where_clause) = ast.generics.split_for_impl();

    let (mapping_impl, struct_body_mapped) = match &struct_data.fields {
        syn::Fields::Unit => {
            return Err(span_compile_error!(span => "Unit structs are not supported"));
        }
        syn::Fields::Named(fields) => {
            let fields_map = fields.named.iter().map(|field| {
                let name = &field.ident;
                let ty = &field.ty;

                quote! { #name: <#ty as #trait_path>::from_bind(bind.#name) }
            });

            let fields_struct = fields.named.iter().map(|field| {
            let name = &field.ident;
            let ty = &field.ty;

            let serde_attrs: Vec<_> = field.attrs.iter()
                .filter(|&attr| matches!(&attr.meta, syn::Meta::List(meta_list) if meta_list.path.is_ident("serde")))
                .collect();

            quote! { #(#serde_attrs)* #name: <#ty as #trait_path>::Bind }
        });

            let mapping = quote! {
                #mapping_name {
                    #(#fields_map,)*
                }
            };

            let struct_body_mapped = quote! {
                {
                    #(#fields_struct,)*
                }
            };

            (mapping, struct_body_mapped)
        }

        syn::Fields::Unnamed(fields) => {
            let fields_map = fields.unnamed.iter().enumerate().map(|(i, field)| {
                let ty = &field.ty;
                let index = Index {
                    index: i as u32,
                    span: field.span(),
                };

                quote! { <#ty as #trait_path>::from_bind(bind.#index) }
            });

            let fields_struct = fields.unnamed.iter().map(|field| {
                let ty = &field.ty;
                quote! { <#ty as #trait_path>::Bind }
            });

            let mapping = quote! {
                #mapping_name(
                    #(#fields_map,)*
                )
            };

            let struct_body_mapped = quote! {
                (
                    #(#fields_struct,)*
                );
            };

            (mapping, struct_body_mapped)
        }
    };

    let result = quote! {
        #[derive(serde::Deserialize)]
        #vis struct #mapping_name #og_ty_generics #og_where_clause #struct_body_mapped

        impl #og_impl_generics #trait_path for #thing_name #og_ty_generics #og_where_clause {
            type Bind = #mapping_name #og_ty_generics;
            fn from_bind(bind: Self::Bind) -> Self {
                #mapping_impl
            }
        }
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_unnamed() {
        let input: syn::DeriveInput = syn::parse_quote! {
            pub struct Foo(u32);
        };

        let output = from_surreal_db_derive(input);
        let expected = quote! {
            #[derive(serde::Deserialize)]
            pub struct SurrealFoo(<u32 as FromSurrealBind>::Bind,);
            impl FromSurrealBind for Foo {
                type Bind = SurrealFoo;
                fn from_bind(bind: Self::Bind) -> Self {
                    SurrealFoo(<u32 as FromSurrealBind>::from_bind(bind.0),)
                }
            }
        };

        assert!(output.is_ok(), "{:?}", output.unwrap_err());
        assert_eq!(output.unwrap().to_string(), expected.to_string());
    }

    #[test]
    fn generates_named() {
        let input: syn::DeriveInput = syn::parse_quote! {
            pub struct Foo {
                bar: u32,
            }
        };

        let output = from_surreal_db_derive(input);
        let expected = quote! {
            #[derive(serde::Deserialize)]
            pub struct SurrealFoo {
                bar: <u32 as FromSurrealBind>::Bind,
            }
            impl FromSurrealBind for Foo {
                type Bind = SurrealFoo;
                fn from_bind(bind: Self::Bind) -> Self {
                    SurrealFoo {
                        bar: <u32 as FromSurrealBind>::from_bind(bind.bar),
                    }
                }
            }
        };

        assert!(output.is_ok(), "{:?}", output.unwrap_err());
        assert_eq!(output.unwrap().to_string(), expected.to_string());
    }

    #[test]
    fn replicates_serde_flatten_attrs() {
        let input: syn::DeriveInput = syn::parse_quote! {
            pub struct Foo {
                #[serde(flatten)]
                inner: FooInner,
            }
        };

        let output = from_surreal_db_derive(input);
        let expected = quote! {
            #[derive(serde::Deserialize)]
            pub struct SurrealFoo {
                #[serde(flatten)]
                inner: <FooInner as FromSurrealBind>::Bind,
            }
            impl FromSurrealBind for Foo {
                type Bind = SurrealFoo;
                fn from_bind(bind: Self::Bind) -> Self {
                    SurrealFoo {
                        inner: <FooInner as FromSurrealBind>::from_bind(bind.inner),
                    }
                }
            }
        };

        assert!(output.is_ok(), "{:?}", output.unwrap_err());
        assert_eq!(output.unwrap().to_string(), expected.to_string());
    }
}
