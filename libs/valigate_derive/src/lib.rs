use proc_macro::TokenStream;
use quote::{format_ident, quote};

#[proc_macro]
pub fn impl_gates_tup(_tokens: TokenStream) -> TokenStream {
    let impls = (2..=12).map(do_impl_gates_tup);
    quote! {
        #(#impls)*
    }
    .into()
}

fn do_impl_gates_tup(limit: usize) -> proc_macro2::TokenStream {
    let clauses = (1..limit)
        .map(|n| (format_ident!("G{}", n), format_ident!("G{}", n - 1)))
        .map(|(gcurrent, gprev)| {
            quote! {
                #gcurrent: Gate<#gprev::Out>
            }
        });

    let last_gate = format_ident!("G{}", limit - 1);
    let gates_ty = (1..limit)
        .map(|n| format_ident!("G{}", n))
        .collect::<Vec<_>>();
    let gates_lets = (1..limit)
        .map(|n| format_ident!("g{}", n))
        .collect::<Vec<_>>();
    let gates_errs = (1..limit)
        .map(|n| format_ident!("gate{}_err", n))
        .collect::<Vec<_>>();

    let nones = (1..limit)
        .map(|_| {
            quote! {
                None
            }
        })
        .collect::<Vec<_>>();

    let all_errs = quote! {
        gate0_err, #(#gates_errs),*
    };

    let implement = quote! {
        impl<I, G0: Gate<I>, #(#clauses),*> Gate<I> for (G0, #(#gates_ty),*) {
            type Out = #last_gate::Out;
            type Err = GateErrors<(Option<G0::Err>, #(Option<#gates_ty::Err>),*)>;

            fn parse(self, input: I) -> GateResult<Self::Out, Self::Err> {
                let (g0, #(#gates_lets),*) = self;

                let (out, gate0_err) = match g0.parse(input) {
                    GateResult::Ok(out) => (out, None),
                    GateResult::ErrCut(err) => return GateResult::ErrCut(GateErrors((Some(err), #(#nones),*))),
                    GateResult::ErrPass(out, err) => (out, Some(err)),
                };

                #(let mut #gates_errs = None;)*
                #(
                    let out = match #gates_lets.parse(out) {
                        GateResult::Ok(out) => {
                            out
                        },
                        GateResult::ErrCut(err) => {
                            #gates_errs = Some(err);
                            return GateResult::ErrCut(GateErrors((#all_errs)));
                        }
                        GateResult::ErrPass(out, err) => {
                            #gates_errs = Some(err);
                            out
                        },
                    };
                )*

                match (gate0_err, #(#gates_errs),*) {
                    (None, #(#nones),*) => GateResult::Ok(out),
                    errs => GateResult::ErrPass(out, GateErrors(errs)),
                }
            }
        }
    };

    implement
}

#[proc_macro_derive(Valid, attributes(valid))]
pub fn derive_valid(tokens: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(tokens as syn::DeriveInput);
    match derive_valid::do_derive_valid(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => proc_macro2::TokenStream::from(err).into(),
    }
}

mod derive_valid {
    use std::str::FromStr;

    use derime::CompileError;
    use quote::quote;
    use syn::{FieldsUnnamed, spanned::Spanned};

    #[derive(Default)]
    enum Usage {
        Crate,
        #[default]
        Lib,
    }

    struct UsageParseErr;
    impl std::fmt::Display for UsageParseErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Invalid usage, expected \"crate\" or \"lib\"")
        }
    }

    impl FromStr for Usage {
        type Err = UsageParseErr;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "crate" => Ok(Usage::Crate),
                "lib" => Ok(Usage::Lib),
                _ => Err(UsageParseErr),
            }
        }
    }

    impl derime::ReprValue for Usage {
        fn repr() -> &'static str {
            "crate|lib"
        }
    }

    pub fn do_derive_valid(
        derive: syn::DeriveInput,
    ) -> Result<proc_macro2::TokenStream, CompileError> {
        let span = derive.span();
        let (usage, gate, serde) = derime::parse_attrs(
            "valid",
            &derive.attrs,
            (
                derime::OptionalAttr((
                    derime::KeyIdent("usage"),
                    derime::IdentValueParser::<Usage>::default(),
                )),
                derime::OptionalAttr((derime::KeyIdent("gate"), derime::ExprParser)),
                derime::OptionalAttr((derime::KeyIdent("serde"), derime::BoolParser)),
            ),
        )?;

        let usage = usage.unwrap_or_default();
        let path = match usage {
            Usage::Crate => quote! { crate },
            Usage::Lib => quote! { valigate },
        };
        let ident = derive.ident;

        let serde_top_attrs = if serde.unwrap_or(false) {
            let attrs: Vec<_> = derive
                .attrs
                .iter()
                .filter(|attr| match &attr.meta {
                    syn::Meta::Path(path) => path.is_ident("serde"),
                    syn::Meta::List(list) => list.path.is_ident("serde"),
                    _ => false,
                })
                .cloned()
                .collect();

            Some(attrs)
        } else {
            None
        };

        let fields = match derive.data {
            syn::Data::Struct(syn::DataStruct {
                fields: syn::Fields::Unnamed(FieldsUnnamed { unnamed, .. }),
                ..
            }) => unnamed,
            syn::Data::Enum(data) => return enum_derive(span, path, serde_top_attrs.as_deref(), ident, data),
            data => {
                return multi_field_derive(
                    span,
                    path,
                    serde_top_attrs.as_deref(),
                    ident,
                    gate,
                    data,
                );
            }
        };

        let field = match fields.into_iter().next() {
            Some(field) => field,
            _ => {
                return Err(
                    derime::span_compile_error!(span => "Only single unnamed structs are supported for now!"),
                );
            }
        };
        let input = field.ty.clone();

        Ok(single_field_derive(path, ident, gate, input))
    }

    fn single_field_derive(
        path: proc_macro2::TokenStream,
        ident: proc_macro2::Ident,
        gate: Option<syn::Expr>,
        input: syn::Type,
    ) -> proc_macro2::TokenStream {
        let gate = gate.unwrap_or_else(|| syn::parse_quote!( #path::gates::NoopField ));
        quote! {
            impl #path::Valid for #ident {
                type In = #input;

                fn parse(input: Self::In) -> Result<Self, #path::Error> {
                    #path::field_pipe(input, #gate).map(#ident)
                }
            }
        }
    }

    fn multi_field_derive(
        span: proc_macro2::Span,
        path: proc_macro2::TokenStream,
        serde_top_attrs: Option<&[syn::Attribute]>,
        ident: proc_macro2::Ident,
        gate: Option<syn::Expr>,
        data: syn::Data,
    ) -> Result<proc_macro2::TokenStream, CompileError> {
        let input_ident = quote::format_ident!("{}Input", &ident);
        let custom_gate_ident = quote::format_ident!("{}Gate", &ident);
        let custom_err_ident = quote::format_ident!("{}Error", &ident);

        let gate: syn::Expr = match gate {
            Some(gate) => syn::parse_quote!((#custom_gate_ident, #gate)),
            None => syn::parse_quote!(#custom_gate_ident),
        };

        let fields = match data {
            syn::Data::Struct(syn::DataStruct {
                fields: syn::Fields::Named(syn::FieldsNamed { named, .. }),
                ..
            }) => named,
            _ => {
                return Err(
                    derime::span_compile_error!(span => "Only structs with named fields are supported for now!"),
                );
            }
        };

        let fields_declaration = fields.iter().map(|field| {
            let ident = field.ident.as_ref().unwrap();
            let attrs = &field.attrs;
            let ty = option_type(&field.ty).unwrap_or(&field.ty);

            quote! {
                #(#attrs)*
                #ident: Option<<#ty as #path::Valid>::In>,
            }
        });

        fn option_type(ty: &syn::Type) -> Option<&syn::Type> {
            let syn::Type::Path(ty) = ty else { return None };
            if ty.qself.is_some() {
                return None;
            }

            let ty = &ty.path;

            if ty.segments.is_empty() || ty.segments.last().unwrap().ident != "Option" {
                return None;
            }

            if !(ty.segments.len() == 1
                || (ty.segments.len() == 3
                    && ["core", "std"].contains(&ty.segments[0].ident.to_string().as_str())
                    && ty.segments[1].ident == "option"))
            {
                return None;
            }

            let last_segment = ty.segments.last().unwrap();
            let syn::PathArguments::AngleBracketed(generics) = &last_segment.arguments else {
                return None;
            };
            if generics.args.len() != 1 {
                return None;
            }
            let syn::GenericArgument::Type(inner_type) = &generics.args[0] else {
                return None;
            };

            Some(inner_type)
        }

        let names_match = fields.iter().map(|field| {
            let ident = field.ident.as_ref().unwrap();

            if option_type(&field.ty).is_some() {
                quote! { #ident }
            } else {
                quote! { Some(#ident) }
            }
        });

        let report_missings = fields.iter().map(|field| {
            let name = field.ident.as_ref().unwrap();
            if option_type(&field.ty).is_some() {
                quote! {}
            }
            else {
                quote! { let error = #path::FieldErrors::from_one(#path::MissingField(stringify!(#name))); }
            }
        });

        let names = fields
            .iter()
            .map(|field| field.ident.as_ref().unwrap())
            .cloned()
            .collect::<Vec<_>>();

        let types = fields
            .iter()
            .map(|field| option_type(&field.ty).unwrap_or(&field.ty).clone())
            .collect::<Vec<_>>();

        let derive_serde = if serde_top_attrs.is_some() {
            quote! { #[derive(serde::Deserialize)] }
        } else {
            quote! {}
        };

        let empty = [];
        let serde_top_attrs = serde_top_attrs.unwrap_or(&empty);

        let derived = quote! {
            #derive_serde
            #(#serde_top_attrs)*
            pub struct #input_ident {
                #(#fields_declaration)*
            }

            struct #custom_gate_ident;

            #[derive(Default)]
            struct #custom_err_ident {
                #(#names: #path::MaybeFieldError<#path::Error>),*
            }

            impl #path::CollectsErrors for #custom_err_ident {
                type Errors = #path::MapErrors;

                fn collect_errs(self, map: &mut Self::Errors) {
                    #(
                        match self.#names {
                            #path::MaybeFieldError::Invalid(e) => {
                                map.insert_field(stringify!(#names), e);
                            }
                            #path::MaybeFieldError::Missing => {
                                let error = #path::FieldErrors::from_one(#path::MissingField(stringify!(#names)));
                                map.insert_field(stringify!(#names), error.into());
                            }
                            _ => {}
                        }
                    )*
                }
            }

            impl #path::Gate<#input_ident> for #custom_gate_ident {
                type Out = #ident;
                type Err = #custom_err_ident;
                fn parse(self, input: #input_ident) -> #path::GateResult<Self::Out, Self::Err> {
                    let mut error = #custom_err_ident::default();

                    #(
                        let #names = match input.#names {
                            Some(v) => match <#types as #path::Valid>::parse(v) {
                                Ok(v) => Some(v),
                                Err(e) => {
                                    error.#names = #path::MaybeFieldError::Invalid(e);
                                    None
                                }
                            },
                            None => {
                                #report_missings
                                None
                            }
                        };
                    )*

                    match (#(#names,)*) {
                        (#( #names_match,)*) => #path::GateResult::Ok(#ident { #(#names,)* }),
                        #[allow(unreachable_pattern)]
                        _ => #path::GateResult::ErrCut(error),
                    }
                }
            }

            impl #path::Valid for #ident {
                type In = #input_ident;

                fn parse(input: Self::In) -> Result<Self, #path::Error> {
                    #path::field_pipe(input, #gate)
                }
            }

        };

        Ok(quote! {
            const _: () = {
                #derived
            };
        })
    }

    fn enum_derive(
        span: proc_macro2::Span,
        path: proc_macro2::TokenStream,
        serde_top_attrs: Option<&[syn::Attribute]>,
        ident: proc_macro2::Ident,
        data: syn::DataEnum,
    ) -> Result<proc_macro2::TokenStream, CompileError> {
        let input_ident = quote::format_ident!("{}Input", &ident);

        struct VariantSingle {
            ident: syn::Ident,
            field: syn::Field,
        }

        let variants = data.variants.iter().map(|v| {
            let field = match &v.fields {
                syn::Fields::Unnamed(FieldsUnnamed { unnamed, .. }) if unnamed.len() == 1 => {
                    unnamed.iter().next().unwrap()
                }
                _ => return Err(derime::span_compile_error!(span => "Only single unnamed structs are supported for now!")),
            };

            Ok(VariantSingle {
                ident: v.ident.clone(),                
                field: field.clone(),
            })
        }).collect::<Result<Vec<_>, _>>()?;

        let input_variants = data.variants.iter().zip(variants.iter()).map(|(variant, single)| {
            let ty = &single.field.ty;
            let mut field = single.field.clone();
            field.ty = syn::parse_quote!(<#ty as #path::Valid>::In);

            let mut modified = variant.clone();
            modified.fields = syn::Fields::Unnamed(syn::parse_quote!((#field)));

            modified
        });

        let serde_top_attrs = serde_top_attrs.unwrap_or(&[]);
        let variants_mapping = variants.iter().map(|v| {
            let variant_ident = &v.ident;
            let ty = &v.field.ty;

            quote! {
                #input_ident::#variant_ident(v) => #ident::#variant_ident(
                    <#ty as #path::Valid>::parse(v)?
                ),
            }
        });

        quote! {
            #(#serde_top_attrs)*
            pub enum #input_ident {
                #(#input_variants)*
            }

            impl #path::Valid for #ident {
                type In = #input_ident;

                fn parse(input: Self::In) -> Result<Self, #path::Error> {
                    Ok(match input {
                        #(#variants_mapping)*
                    })
                }
            }
        };

        todo!()
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    #[test]
    fn it_works() {
        let result = super::do_impl_gates_tup(2);
        assert_eq!(
            result.to_string(),
            quote! {
                impl<I, G0: Gate<I>, G1: Gate<G0::Out> > Gate<I> for (G0, G1) {

                }
            }
            .to_string()
        );
    }

    #[test]
    fn a() {
        let result = crate::derive_valid::do_derive_valid(syn::parse_quote! {
            struct A {
                a: i32,
            }
        })
        .unwrap();

        assert_eq!(
            result.to_string(),
            quote! {
                impl Valid for A {
                    type In = A;
                    fn parse(input: Self::In) -> Result<Self, Error> {
                        Ok(input)
                    }
                }
            }
            .to_string()
        );
    }
}
