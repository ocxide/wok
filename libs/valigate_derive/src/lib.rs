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
        let (usage, gate) = derime::parse_attrs(
            "valid",
            &derive.attrs,
            (
                derime::OptionalAttr((
                    derime::KeyIdent("usage"),
                    derime::IdentValueParser::<Usage>::default(),
                )),
                derime::OptionalAttr((derime::KeyIdent("gate"), derime::ExprParser)),
            ),
        )?;

        let usage = usage.unwrap_or_default();
        let path = match usage {
            Usage::Crate => quote! { crate },
            Usage::Lib => quote! { valigate },
        };
        let ident = derive.ident;

        let fields = match derive.data {
            syn::Data::Struct(syn::DataStruct {
                fields: syn::Fields::Unnamed(FieldsUnnamed { unnamed, .. }),
                ..
            }) => unnamed,
            data => return multi_field_derive(span, path, ident, gate, data),
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

        let names = fields
            .iter()
            .map(|field| field.ident.as_ref().unwrap())
            .cloned()
            .collect::<Vec<_>>();

        let types = fields
            .iter()
            .map(|field| field.ty.clone())
            .collect::<Vec<_>>();

        let derived = quote! {
            pub struct #input_ident {
                #(#names: Option<<#types as Valid>::In>),*
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
                            Some(v) => match <#types as Valid>::parse(v) {
                                Ok(v) => Some(v),
                                Err(e) => {
                                    error.#names = #path::MaybeFieldError::Invalid(e);
                                    None
                                }
                            },
                            None => {
                                error.#names = #path::MaybeFieldError::Missing;
                                None
                            }
                        };
                    )*

                    match (#(#names,)*) {
                        (#(Some(#names),)*) => #path::GateResult::Ok(#ident { #(#names,)* }),
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
