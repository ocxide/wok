use quote::quote;
use syn::{
    Expr, Index, Lifetime, MetaNameValue, Token, parse::Parser, punctuated::Punctuated,
    spanned::Spanned,
};

#[proc_macro_derive(Param, attributes(param))]
pub fn param_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match do_param_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into(),
    }
}

fn do_param_derive(ast: syn::DeriveInput) -> Result<proc_macro2::TokenStream, CompileError> {
    let span = ast.span();
    let struct_data = match ast.data {
        syn::Data::Struct(data) => data,
        _ => {
            return Err(span_compile_error!(span => "Param can only be derived on structs"));
        }
    };

    let thing_name = &ast.ident;
    let trait_path = quote! { Param };

    enum Usage {
        Core,
        Lib,
        Crate,
    }

    let usage = || {
        let Some(attr) = &ast.attrs.first() else {
            return Ok(Usage::Crate);
        };

        let tokens = match &attr.meta {
            syn::Meta::List(meta_list) => &meta_list.tokens,
            _ => return Err(span_compile_error!(attr.span() => "Expected #[param(...)]")),
        };

        let keyvalues = Punctuated::<MetaNameValue, Token![,]>::parse_terminated
            .parse2(tokens.clone())
            .map_err(|_| span_compile_error!(attr.span() => "Invalid attribute syntax"))?;

        let usage = match keyvalues.first() {
            Some(MetaNameValue {
                path,
                value: Expr::Path(expr_path),
                ..
            }) if path.is_ident("usage") => {
                if expr_path.path.is_ident("core") {
                    Usage::Core
                } else if expr_path.path.is_ident("lib") {
                    Usage::Lib
                } else if expr_path.path.is_ident("crate") {
                    Usage::Crate
                } else {
                    return Err(
                        span_compile_error!(attr.span() => "useages supported are `core`, `lib` and `crate`"),
                    );
                }
            }
            None => return Ok(Usage::Crate),
            _ => {
                return Err(span_compile_error!(attr.span() => "Expected #[param(path = ...)]"));
            }
        };

        Ok(usage)
    };
    let usage = usage()?;

    let rw = match usage {
        Usage::Core => quote! { crate::world::SystemLock },
        Usage::Lib => quote! { lump_core::world::SystemLock },
        Usage::Crate => quote! { lump::world::SystemLock },
    };

    let world = match usage {
        Usage::Core => quote! { crate::prelude::WorldState },
        Usage::Lib => quote! { lump_core::prelude::WorldState },
        Usage::Crate => quote! { lump::prelude::WorldState },
    };

    let fields_tyes = match &struct_data.fields {
        syn::Fields::Named(named) => named.named.iter().map(|field| field.ty.clone()).collect(),
        syn::Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .map(|field| field.ty.clone())
            .collect(),
        syn::Fields::Unit => vec![],
    };

    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let from_owned_impl = match &struct_data.fields {
        syn::Fields::Unit => quote! { #thing_name },
        syn::Fields::Named(fields) => {
            let fields_map = fields.named.iter().enumerate().map(|(i, field)| {
                let name = &field.ident;
                let ty = &field.ty;
                let index = Index {
                    index: i as u32,
                    span: field.span(),
                };

                quote! { #name: <#ty as #trait_path>::from_owned(&owned.#index) }
            });

            quote! {
                #thing_name {
                    #(#fields_map,)*
                }
            }
        }

        syn::Fields::Unnamed(fields) => {
            let fields_map = fields.unnamed.iter().enumerate().map(|(i, field)| {
                let ty = &field.ty;
                let index = Index {
                    index: i as u32,
                    span: field.span(),
                };

                quote! { <#ty as #trait_path>::from_owned(&owned.#index) }
            });

            quote! {
                #thing_name(
                    #(#fields_map,)*
                )
            }
        }
    };

    let mut reborrow_generics = ast.generics.clone();

    let altern_lifetime = {
        let ident = match &ast.generics.lifetimes().next() {
            Some(lifetime) if lifetime.lifetime.ident == "p" => "'w",
            _ => "'p",
        };

        Lifetime::new(ident, proc_macro2::Span::call_site())
    };

    if let Some(lifetime) = reborrow_generics.lifetimes_mut().next() {
        lifetime.lifetime = altern_lifetime.clone();
    }
    let (_, reborrow_ty_generics, _) = reborrow_generics.split_for_impl();

    let output = quote! {
        impl #impl_generics #trait_path for #thing_name #ty_generics #where_clause {
            type Owned = ( #(<#fields_tyes as #trait_path>::Owned,)* );
            type AsRef<#altern_lifetime> = #thing_name #reborrow_ty_generics;

            fn init(rw: &mut #rw) {
                #(
                    <#fields_tyes as #trait_path>::init(rw);
                )*
            }

            fn get(world: &#world) -> Self::Owned {
                (
                    #(
                        <#fields_tyes as #trait_path>::get(world),
                    )*
                )
            }

            fn from_owned<#altern_lifetime>(owned: &#altern_lifetime Self::Owned) -> Self::AsRef<#altern_lifetime> {
                #from_owned_impl
            }
        }
    };

    Ok(output)
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

#[test]
fn empty() {
    let input: syn::DeriveInput = syn::parse_quote! {
        struct Foo;
    };

    let output = do_param_derive(input);
    assert!(output.is_ok(), "{:?}", output.unwrap_err());
}

#[test]
fn single() {
    let input: syn::DeriveInput = syn::parse_quote! {
        struct Foo<'w>(Bar<'w>);
    };

    let output = do_param_derive(input);
    assert!(output.is_ok(), "{:?}", output.unwrap_err());
    assert_eq!(
        output.unwrap().to_string(),
        quote! {
            impl<'w> Param for Foo<'w> {
                type Owned = ( <Bar<'w> as Param>::Owned, );
                type AsRef<'p> = Foo<'p>;

                fn init(rw: &mut lump::world::SystemLock) {
                    <Bar<'w> as Param>::init(rw);
                }

                fn get(world: &lump::prelude::WorldState) -> Self::Owned {
                    ( <Bar<'w> as Param>::get(world), )
                }

                fn from_owned<'p>(owned: &'p Self::Owned) -> Self::AsRef<'p> {
                    Foo( <Bar<'w> as Param>::from_owned(&owned.0), )
                }
            }
        }
        .to_string()
    );
}

#[test]
fn single_for_core() {
    let input: syn::DeriveInput = syn::parse_quote! {
        #[param(usage = core)]
        struct Foo<'w>(Bar<'w>);
    };

    let output = do_param_derive(input);
    assert!(output.is_ok(), "{:?}", output.unwrap_err());
    assert_eq!(
        output.unwrap().to_string(),
        quote! {
            impl<'w> Param for Foo<'w> {
                type Owned = ( <Bar<'w> as Param>::Owned, );
                type AsRef<'p> = Foo<'p>;

                fn init(rw: &mut crate::world::SystemLock) {
                    <Bar<'w> as Param>::init(rw);
                }

                fn get(world: &crate::prelude::WorldState) -> Self::Owned {
                    ( <Bar<'w> as Param>::get(world), )
                }

                fn from_owned<'p>(owned: &'p Self::Owned) -> Self::AsRef<'p> {
                    Foo( <Bar<'w> as Param>::from_owned(&owned.0), )
                }
            }
        }
        .to_string()
    );
}

#[test]
fn single_for_lib() {
    let input: syn::DeriveInput = syn::parse_quote! {
        #[param(usage = lib)]
        struct Foo<'w>(Bar<'w>);
    };

    let output = do_param_derive(input);
    assert!(output.is_ok(), "{:?}", output.unwrap_err());
    assert_eq!(
        output.unwrap().to_string(),
        quote! {
            impl<'w> Param for Foo<'w> {
                type Owned = ( <Bar<'w> as Param>::Owned, );
                type AsRef<'p> = Foo<'p>;

                fn init(rw: &mut lump_core::world::SystemLock) {
                    <Bar<'w> as Param>::init(rw);
                }

                fn get(world: &lump_core::prelude::WorldState) -> Self::Owned {
                    ( <Bar<'w> as Param>::get(world), )
                }

                fn from_owned<'p>(owned: &'p Self::Owned) -> Self::AsRef<'p> {
                    Foo( <Bar<'w> as Param>::from_owned(&owned.0), )
                }
            }
        }
        .to_string()
    );
}
