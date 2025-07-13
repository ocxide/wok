use proc_macro2::Ident;
use quote::quote;
use syn::{
    Expr, Index, MetaNameValue, Token, parse::Parser, punctuated::Punctuated, spanned::Spanned,
};

#[proc_macro_derive(AsSurrealBind, attributes(surreal_bind))]
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

    dbg!(tokens.to_string());
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
                span_compile_error!(span => "AsSurrealBind can only be derived on structs"),
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
    let trait_path = quote! { AsSurrealBind };

    let vis = &ast.vis;

    let mut bind_generics = ast.generics.clone();
    bind_generics.params.push(syn::parse_quote! { 'b });
    let (_, bind_ty_generics, bind_where_clause) = bind_generics.split_for_impl();

    let (og_impl_generics, og_ty_generics, og_where_clause) = ast.generics.split_for_impl();

    let (mapping_impl, struct_body_mapped) = match &struct_data.fields {
        syn::Fields::Unit => return Err(span_compile_error!(span => "Unit structs are not supported")),
        syn::Fields::Named(fields) => {
            let fields_map = fields.named.iter().map(|field| {
                let name = &field.ident;
                let ty = &field.ty;

                quote! { #name: <#ty as #trait_path>::as_bind(&self.#name) }
            });

            let fields_struct = fields.named.iter().map(|field| {
                let name = &field.ident;
                let ty = &field.ty;

                let serde_attrs: Vec<_> = field.attrs.iter()
                    .filter(|&attr| matches!(&attr.meta, syn::Meta::List(meta_list) if meta_list.path.is_ident("serde")))
                    .collect();

                quote! { #(#serde_attrs)* #name: <#ty as #trait_path>::Bind<'b> }
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

                quote! { <#ty as #trait_path>::as_bind(&self.#index) }
            });

            let fields_struct = fields.unnamed.iter().map(|field| {
                let ty = &field.ty;
                quote! { <#ty as #trait_path>::Bind<'b> }
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
        #[derive(serde::Serialize)]
        #vis struct #mapping_name #bind_ty_generics #bind_where_clause #struct_body_mapped

        impl #og_impl_generics #trait_path for #thing_name #og_ty_generics #og_where_clause {
            type Bind<'b> = #mapping_name #bind_ty_generics;
            fn as_bind(&self) -> Self::Bind<'_> {
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
    fn generates_unnamed() {
        let input: syn::DeriveInput = syn::parse_quote! {
            pub struct Foo(u32);
        };

        let output = do_surreal_bind_derive(input);
        let expected = quote! {
            #[derive(serde::Serialize)]
            pub struct SurrealFoo<'b>(<u32 as AsSurrealBind>::Bind<'b>,);
            impl AsSurrealBind for Foo {
                type Bind<'b> = SurrealFoo<'b>;
                fn as_bind(&self) -> Self::Bind<'_> {
                    SurrealFoo(<u32 as AsSurrealBind>::as_bind(&self.0),)
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
            #[derive(serde::Serialize)]
            pub struct SurrealFoo<'b> {
                bar: <u32 as AsSurrealBind>::Bind<'b>,
            }
            impl AsSurrealBind for Foo {
                type Bind<'b> = SurrealFoo<'b>;
                fn as_bind(&self) -> Self::Bind<'_> {
                    SurrealFoo {
                        bar: <u32 as AsSurrealBind>::as_bind(&self.bar),
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

        let output = do_surreal_bind_derive(input);
        let expected = quote! {
            #[derive(serde::Serialize)]
            pub struct SurrealFoo<'b> {
                #[serde(flatten)]
                inner: <FooInner as AsSurrealBind>::Bind<'b>,
            }
            impl AsSurrealBind for Foo {
                type Bind<'b> = SurrealFoo<'b>;
                fn as_bind(&self) -> Self::Bind<'_> {
                    SurrealFoo {
                        inner: <FooInner as AsSurrealBind>::as_bind(&self.inner),
                    }
                }
            }
        };

        assert!(output.is_ok(), "{:?}", output.unwrap_err());
        assert_eq!(output.unwrap().to_string(), expected.to_string());
    }
}
