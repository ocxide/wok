use crate::{CompileError, derime, span_compile_error};
use quote::quote;
use syn::{Index, Lifetime, spanned::Spanned};

#[derive(Debug, PartialEq, Eq, Default)]
pub enum Usage {
    Core,
    Lib,
    #[default]
    Crate,
}

impl derime::ReprValue for Usage {
    fn repr() -> &'static str {
        "core|lib|crate"
    }
}

pub struct StaticErr(&'static str);

impl std::fmt::Display for StaticErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Usage {
    type Err = StaticErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "core" => Ok(Usage::Core),
            "lib" => Ok(Usage::Lib),
            "crate" => Ok(Usage::Crate),
            _ => Err(StaticErr("expected core|lib|crate")),
        }
    }
}

fn get_usage(attrs: &[syn::Attribute]) -> Result<Usage, CompileError> {
    let usage = derime::parse_attrs(
        "params",
        attrs,
        derime::OptionalAttr((
            derime::KeyIdent("usage"),
            derime::IdentValueParser::<Usage>::new(),
        )),
    )?
    .unwrap_or_default();

    Ok(usage)
}

pub fn do_param_derive(ast: syn::DeriveInput) -> Result<proc_macro2::TokenStream, CompileError> {
    let span = ast.span();
    let struct_data = match ast.data {
        syn::Data::Struct(data) => data,
        _ => {
            return Err(span_compile_error!(span => "Param can only be derived on structs"));
        }
    };

    let thing_name = &ast.ident;

    let usage = get_usage(&ast.attrs)?;

    let param_path = match usage {
        Usage::Core => quote! { crate::param },
        Usage::Lib => quote! { wok_core::prelude },
        Usage::Crate => quote! { wok::prelude },
    };

    let trait_path = quote! { #param_path::Param };

    let rw = match usage {
        Usage::Core => quote! { crate::world::SystemLock },
        Usage::Lib => quote! { wok_core::world::SystemLock },
        Usage::Crate => quote! { wok::prelude::SystemLock },
    };

    let world = match usage {
        Usage::Core => quote! { crate::prelude::UnsafeMutState },
        Usage::Lib => quote! { wok_core::prelude::UnsafeMutState },
        Usage::Crate => quote! { wok::prelude::UnsafeMutState },
    };

    let field_types = match &struct_data.fields {
        syn::Fields::Unit => vec![],
        syn::Fields::Named(fields) => fields.named.iter().map(|field| &field.ty).collect(),
        syn::Fields::Unnamed(fields) => fields.unnamed.iter().map(|field| &field.ty).collect(),
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
                    quote! { <#ty as #trait_path>::get_owned(state) }
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

                quote! { <#ty as #trait_path>::get_owned(state) }
            });

            quote! {
                (
                    #(#fields_map,)*
                )
            }
        }
    };

    let mut reborrow_generics = ast.generics.clone();

    let mut readonly_generics = ast.generics.clone();
    let readonly_where_clause = readonly_generics.make_where_clause();
    for ty in &field_types {
        readonly_where_clause
            .predicates
            .push(syn::parse_quote! { #ty: #param_path::ReadonlyParam });
    }

    let mut borrow_mut_generics = ast.generics.clone();
    let borrow_mut_where_clause = borrow_mut_generics.make_where_clause();
    for ty in &field_types {
        borrow_mut_where_clause
            .predicates
            .push(syn::parse_quote! { #ty: #param_path::BorrowMutParam });
    }

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

            unsafe fn get_owned(state: &#world) -> Self::Owned {
                unsafe { #get_owned_impl }
            }

           unsafe fn get_ref(state: &#world) -> Self::AsRef<'_> {
                unsafe { #get_ref_impl }
            }

            fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
                #from_owned_impl
            }
        }

        // Only impl if all fields comply
        unsafe impl #impl_generics #param_path::BorrowMutParam for #thing_name #ty_generics #borrow_mut_where_clause {}
        impl #impl_generics #param_path::ReadonlyParam for #thing_name #ty_generics #readonly_where_clause {}
    };

    Ok(output)
}

#[test]
fn parses_attrs() {
    let input: syn::DeriveInput = syn::parse_quote! {
        #[param(usage = lib)]
        struct Foo;
    };

    let usage = get_usage(input.attrs.as_slice()).unwrap();
    assert_eq!(usage, Usage::Lib);
}

#[test]
fn parser_no_attrs() {
    let input: syn::DeriveInput = syn::parse_quote! {
        struct Foo;
    };

    let usage = get_usage(input.attrs.as_slice()).unwrap();
    assert_eq!(usage, Usage::Crate);
}

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

                unsafe fn get_owned(state: &wok::prelude::UnsafeMutState) -> Self::Owned {
                    unsafe { ( <Bar<'w> as Param>::get_owned(state), ) }
                }

                unsafe fn get_ref(state: &wok::prelude::UnsafeMutState) -> Self::AsRef<'_> {
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

                unsafe fn get_owned(state: &crate::prelude::UnsafeMutState) -> Self::Owned {
                    unsafe { ( <Bar<'w> as Param>::get_owned(state), ) }
                }

                unsafe fn get_ref(state: &crate::prelude::UnsafeMutState) -> Self::AsRef<'_> {
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

                unsafe fn get_owned(state: &wok_core::prelude::UnsafeMutState) -> Self::Owned {
                    unsafe { ( <Bar<'w> as Param>::get_owned(state), ) }
                }

                unsafe fn get_ref(state: &wok_core::prelude::UnsafeMutState) -> Self::AsRef<'_> {
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

            unsafe fn get_owned(state: &crate::prelude::UnsafeMutState) -> Self::Owned {
                unsafe { ( <Bar<'w> as Param>::get_owned(state), Default::default(), ) }
            }

            unsafe fn get_ref(state: &crate::prelude::UnsafeMutState) -> Self::AsRef<'_> {
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
