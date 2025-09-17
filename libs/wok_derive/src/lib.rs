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
        Usage::Lib => quote! { wok_core::world::SystemLock },
        Usage::Crate => quote! { wok::world::SystemLock },
    };

    let world = match usage {
        Usage::Core => quote! { crate::prelude::UnsafeWorldState },
        Usage::Lib => quote! { wok_core::prelude::UnsafeWorldState },
        Usage::Crate => quote! { wok::prelude::UnsafeWorldState },
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

                if has_default_attr(field) {
                    quote! { #name: Default::default() }
                } else {
                    quote! { #name: <#ty as #trait_path>::from_owned(&mut owned.#index) }
                }
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

                quote! { <#ty as #trait_path>::from_owned(&mut owned.#index) }
            });

            quote! {
                #thing_name(
                    #(#fields_map,)*
                )
            }
        }
    };

    fn has_default_attr(field: &syn::Field) -> bool {
        field
            .attrs
            .iter()
            .filter_map(|attr| match &attr.meta {
                syn::Meta::List(meta_list) => Some(meta_list),
                _ => None,
            })
            .any(|list| list.tokens.to_string() == "default")
    }

    let get_ref_impl = match &struct_data.fields {
        syn::Fields::Unit => quote! { #thing_name },
        syn::Fields::Named(fields) => {
            let fields_map = fields.named.iter().map(|field| {
                let name = &field.ident;
                let ty = &field.ty;

                if has_default_attr(field) {
                    quote! { #name: Default::default() }
                } else {
                    quote! { #name: <#ty as #trait_path>::get_ref(state) }
                }
            });

            quote! {
                #thing_name {
                    #(#fields_map,)*
                }
            }
        }

        syn::Fields::Unnamed(fields) => {
            let fields_map = fields.unnamed.iter().map(|field| {
                let ty = &field.ty;

                quote! { <#ty as #trait_path>::get_ref(state) }
            });

            quote! {
                #thing_name(
                    #(#fields_map,)*
                )
            }
        }
    };

    let owned_tuple_ty = match &struct_data.fields {
        syn::Fields::Unit => quote! { () },
        syn::Fields::Named(fields) => {
            let fields_map = fields.named.iter().map(|field| {
                let ty = &field.ty;

                if has_default_attr(field) {
                    quote! { #ty }
                } else {
                    quote! { <#ty as #trait_path>::Owned }
                }
            });

            quote! {
                ( #(#fields_map,)* )
            }
        }

        syn::Fields::Unnamed(fields) => {
            let fields_map = fields.unnamed.iter().map(|field| {
                let ty = &field.ty;

                quote! { <#ty as #trait_path>::Owned }
            });

            quote! {
                ( #(#fields_map,)* )
            }
        }
    };

    let init_impls = match &struct_data.fields {
        syn::Fields::Unit => quote! {},
        syn::Fields::Named(fields) => {
            let fields_map = fields.named.iter().map(|field| {
                let ty = &field.ty;

                if has_default_attr(field) {
                    quote! {}
                } else {
                    quote! { <#ty as #trait_path>::init(rw); }
                }
            });

            quote! {
                #(#fields_map)*
            }
        }

        syn::Fields::Unnamed(fields) => {
            let fields_map = fields.unnamed.iter().map(|field| {
                let ty = &field.ty;

                quote! { <#ty as #trait_path>::init(rw); }
            });

            quote! {
                #(#fields_map)*
            }
        }
    };

    let get_owned_impl = match &struct_data.fields {
        syn::Fields::Unit => quote! { () },
        syn::Fields::Named(fields) => {
            let fields_map = fields.named.iter().map(|field| {
                let ty = &field.ty;

                if has_default_attr(field) {
                    quote! { Default::default() }
                } else {
                    quote! { <#ty as #trait_path>::get(state) }
                }
            });

            quote! {
                (
                    #(#fields_map,)*
                )
            }
        }

        syn::Fields::Unnamed(fields) => {
            let fields_map = fields.unnamed.iter().map(|field| {
                let ty = &field.ty;

                quote! { <#ty as #trait_path>::get(state) }
            });

            quote! {
                (
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
            type Owned = #owned_tuple_ty;
            type AsRef<#altern_lifetime> = #thing_name #reborrow_ty_generics;

            fn init(rw: &mut #rw) {
                #init_impls
            }

            unsafe fn get(state: &#world) -> Self::Owned {
                unsafe { #get_owned_impl }
            }

           unsafe fn get_ref(state: &#world) -> Self::AsRef<'_> {
                unsafe { #get_ref_impl }
            }

            fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
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

                fn init(rw: &mut wok::world::SystemLock) {
                    <Bar<'w> as Param>::init(rw);
                }

                unsafe fn get(state: &wok::prelude::UnsafeWorldState) -> Self::Owned {
                    unsafe { ( <Bar<'w> as Param>::get(state), ) }
                }

                unsafe fn get_ref(state: &wok::prelude::UnsafeWorldState) -> Self::AsRef<'_> {
                    unsafe { Foo( <Bar<'w> as Param>::get_ref(state), ) }
                }

                fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
                    Foo( <Bar<'w> as Param>::from_owned(&mut owned.0), )
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

                unsafe fn get(state: &crate::prelude::UnsafeWorldState) -> Self::Owned {
                    unsafe { ( <Bar<'w> as Param>::get(state), ) }
                }

                unsafe fn get_ref(state: &crate::prelude::UnsafeWorldState) -> Self::AsRef<'_> {
                    unsafe { Foo( <Bar<'w> as Param>::get_ref(state), ) }
                }

                fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
                    Foo( <Bar<'w> as Param>::from_owned(&mut owned.0), )
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

                fn init(rw: &mut wok_core::world::SystemLock) {
                    <Bar<'w> as Param>::init(rw);
                }

                unsafe fn get(state: &wok_core::prelude::UnsafeWorldState) -> Self::Owned {
                    unsafe { ( <Bar<'w> as Param>::get(state), ) }
                }

                unsafe fn get_ref(state: &wok_core::prelude::UnsafeWorldState) -> Self::AsRef<'_> {
                    unsafe { Foo( <Bar<'w> as Param>::get_ref(state), ) }
                }

                fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
                    Foo( <Bar<'w> as Param>::from_owned(&mut owned.0), )
                }

            }
        }
        .to_string()
    );
}

#[test]
fn with_default() {
    let input: syn::DeriveInput = syn::parse_quote! {
        #[param(usage = core)]
        struct Foo<'w> {
            bar: Bar<'w>,

            #[param(default)]
            _marker: std::marker::PhantomData<()>,
        }
    };

    let output = do_param_derive(input);
    assert!(output.is_ok(), "{:?}", output.unwrap_err());
    assert_eq!(
        output.unwrap().to_string(),
        quote! {
            impl<'w> Param for Foo<'w> {
                type Owned = ( <Bar<'w> as Param>::Owned, std::marker::PhantomData<()>, );
                type AsRef<'p> = Foo<'p>;

                fn init(rw: &mut crate::world::SystemLock) {
                    <Bar<'w> as Param>::init(rw);
                }

                unsafe fn get(state: &crate::prelude::UnsafeWorldState) -> Self::Owned {
                    unsafe { ( <Bar<'w> as Param>::get(state), Default::default(), ) }
                }

                unsafe fn get_ref(state: &crate::prelude::UnsafeWorldState) -> Self::AsRef<'_> {
                    unsafe { Foo { bar: <Bar<'w> as Param>::get_ref(state), _marker: Default::default(), } }
                }

                fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
                    Foo {
                        bar: <Bar<'w> as Param>::from_owned(&mut owned.0),
                        _marker: Default::default(),
                    }
                }
            }
        }
        .to_string()
    );
}
