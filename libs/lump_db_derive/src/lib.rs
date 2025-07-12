use proc_macro2::Ident;
use quote::quote;
use syn::{
    Expr, Index, MetaNameValue, Token, parse::Parser, punctuated::Punctuated, spanned::Spanned,
};

#[proc_macro_derive(IntoSurrealBind, attributes(surreal_bind))]
pub fn surreal_bind_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match do_surreal_bind_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into(),
    }
}

enum InvalidAttrReason {
    NotScoped,
    Other(CompileError),
}

fn named_attrs(attrs: &[syn::Attribute]) -> Result<Option<MetaNameValue>, InvalidAttrReason> {
    let Some(attr) = attrs.first() else {
        return Ok(None);
    };

    let tokens = match &attr.meta {
        syn::Meta::List(meta_list) => &meta_list.tokens,
        _ => return Err(InvalidAttrReason::NotScoped),
    };

    let keyvalues = Punctuated::<MetaNameValue, Token![,]>::parse_terminated
        .parse2(tokens.clone())
        .map_err(|_| {
            InvalidAttrReason::Other(span_compile_error!(attr.span() => "Invalid attribute syntax"))
        })?;

    match keyvalues.first() {
        Some(meta) => Ok(Some(meta.clone())),
        None => Ok(None),
    }
}

fn do_surreal_bind_derive(ast: syn::DeriveInput) -> Result<proc_macro2::TokenStream, CompileError> {
    let span = ast.span();
    let struct_data = match ast.data {
        syn::Data::Struct(data) => data,
        _ => {
            return Err(
                span_compile_error!(span => "IntoSurrealBind can only be derived on structs"),
            );
        }
    };

    let new_name: String;
    let mapping_name = match named_attrs(&ast.attrs) {
        Ok(Some(MetaNameValue {
            path,
            value: Expr::Path(expr),
            ..
        })) if path.is_ident("name") => match expr.path.get_ident() {
            Some(ident) => ident.clone(),
            None => {
                return Err(span_compile_error!(span => "Expected #[surreal_bind(name = ...)]"));
            }
        },
        Ok(Some(_)) => {
            return Err(span_compile_error!(span => "Expected #[surreal_bind(name = ...)]"));
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
    let trait_path = quote! { IntoSurrealBind };

    let vis = &ast.vis;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let (mapping_impl, struct_body_mapped) = match &struct_data.fields {
        syn::Fields::Unit => {
            let mapping = quote! { #mapping_name };
            let struct_mapped = quote! {;};

            (mapping, struct_mapped)
        }
        syn::Fields::Named(fields) => {
            let fields_map = fields.named.iter().map(|field| {
                let name = &field.ident;
                let ty = &field.ty;

                quote! { #name: <#ty as #trait_path>::into_bind(self.#name) }
            });

            let fields_struct = fields.named.iter().map(|field| {
                let name = &field.ident;
                let ty = &field.ty;

                quote! { #name: <#ty as #trait_path>::Bind }
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

                quote! { <#ty as #trait_path>::into_bind(&owned.#index) }
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

            let struct_mapped = quote! {
                struct #mapping_name(
                    #(#fields_struct,)*
                )
            };

            (mapping, struct_mapped)
        }
    };

    let result = quote! {
        #vis struct #mapping_name #ty_generics #where_clause #struct_body_mapped

        impl #impl_generics #trait_path for #thing_name #ty_generics #where_clause {
            type Bind = #mapping_name #ty_generics;
            fn into_bind(self) -> Self::Bind {
                #mapping_impl
            }
        }
    };

    Ok(result)
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

macro_rules! span_compile_error(($span: expr => $msg: expr) => {
    crate::CompileError(quote::quote_spanned! { $span => compile_error!($msg) }.into())
});

pub(crate) use span_compile_error;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let input: syn::DeriveInput = syn::parse_quote! {
            struct Foo;
        };

        let output = do_surreal_bind_derive(input);
        assert!(output.is_ok(), "{:?}", output.unwrap_err());
    }

    #[test]
    fn auto_generates_empty() {
        let input: syn::DeriveInput = syn::parse_quote! {
            struct Foo;
        };

        let output = do_surreal_bind_derive(input);
        let expected = quote! {
            struct SurrealFoo;
            impl IntoSurrealBind for Foo {
                type Bind = SurrealFoo;
                fn into_bind(self) -> Self::Bind {
                    SurrealFoo
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

        let output = do_surreal_bind_derive(input);
        let expected = quote! {
            pub struct SurrealFoo {
                bar: <u32 as IntoSurrealBind>::Bind,
            }
            impl IntoSurrealBind for Foo {
                type Bind = SurrealFoo;
                fn into_bind(self) -> Self::Bind {
                    SurrealFoo {
                        bar: <u32 as IntoSurrealBind>::into_bind(self.bar),
                    }
                }
            }
        };

        assert!(output.is_ok(), "{:?}", output.unwrap_err());
        assert_eq!(output.unwrap().to_string(), expected.to_string());
    }
}
