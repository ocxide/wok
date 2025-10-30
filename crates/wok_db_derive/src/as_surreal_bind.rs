use derime::{span_compile_error, CompileError};
use proc_macro2::Ident;
use quote::quote;
use syn::{Index, spanned::Spanned};

pub fn do_as_surreal_bind_derive(
    ast: syn::DeriveInput,
) -> Result<proc_macro2::TokenStream, CompileError> {
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

    let name = derime::AttributesParser::new("surreal_bind").parse(
        &ast.attrs,
        derime::OptionalAttr((derime::KeyIdent("ser"), derime::StringParser)),
    )?;

    let mapping_name = match name {
        Some(name) => Ident::new(&name, ast.ident.span()),
        None => {
            new_name = format!("Surreal{}Ref", ast.ident);
            Ident::new(&new_name, ast.ident.span())
        }
    };

    let thing_name = &ast.ident;
    let trait_path = quote! { AsSurrealBind };

    let vis = &ast.vis;

    let mut bind_generics = ast.generics.clone();
    bind_generics.params.push(syn::parse_quote! { 'b });
    let (_, bind_ty_generics, bind_where_clause) = bind_generics.split_for_impl();

    let (og_impl_generics, og_ty_generics, og_where_clause) = ast.generics.split_for_impl();

    let (mapping_impl, struct_body_mapped) = match &struct_data.fields {
        syn::Fields::Unit => {
            return Err(span_compile_error!(span => "Unit structs are not supported"));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_unnamed() {
        let input: syn::DeriveInput = syn::parse_quote! {
            pub struct Foo(u32);
        };

        let output = do_as_surreal_bind_derive(input);
        let expected = quote! {
            #[derive(serde::Serialize)]
            pub struct SurrealFooRef<'b>(<u32 as AsSurrealBind>::Bind<'b>,);
            impl AsSurrealBind for Foo {
                type Bind<'b> = SurrealFooRef<'b>;
                fn as_bind(&self) -> Self::Bind<'_> {
                    SurrealFooRef(<u32 as AsSurrealBind>::as_bind(&self.0),)
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

        let output = do_as_surreal_bind_derive(input);
        let expected = quote! {
            #[derive(serde::Serialize)]
            pub struct SurrealFooRef<'b> {
                bar: <u32 as AsSurrealBind>::Bind<'b>,
            }
            impl AsSurrealBind for Foo {
                type Bind<'b> = SurrealFooRef<'b>;
                fn as_bind(&self) -> Self::Bind<'_> {
                    SurrealFooRef {
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

        let output = do_as_surreal_bind_derive(input);
        let expected = quote! {
            #[derive(serde::Serialize)]
            pub struct SurrealFooRef<'b> {
                #[serde(flatten)]
                inner: <FooInner as AsSurrealBind>::Bind<'b>,
            }
            impl AsSurrealBind for Foo {
                type Bind<'b> = SurrealFooRef<'b>;
                fn as_bind(&self) -> Self::Bind<'_> {
                    SurrealFooRef {
                        inner: <FooInner as AsSurrealBind>::as_bind(&self.inner),
                    }
                }
            }
        };

        assert!(output.is_ok(), "{:?}", output.unwrap_err());
        assert_eq!(output.unwrap().to_string(), expected.to_string());
    }
}
