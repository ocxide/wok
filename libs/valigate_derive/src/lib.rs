use std::str::FromStr;

use derime::CompileError;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FieldsUnnamed, spanned::Spanned};

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

    let implement = quote! {
        impl<I, G0: Gate<I>, #(#clauses),*> Gate<I> for (G0, #(#gates_ty),*) {
            type Out = #last_gate::Out;
            type Err = (Option<G0::Err>, #(Option<#gates_ty::Err>),*);

            fn parse(self, input: I) -> (Self::Out, Result<(), Self::Err>) {
                let (g0, #(#gates_lets),*) = self;
                let (out, gate_0_err) = g0.parse(input);

                #(
                    let (out, #gates_errs) = #gates_lets.parse(out);
                )*

                let result = match (gate_0_err.err(), #(#gates_errs.err()),*) {
                    (None, #(#nones),*) => Ok(()),
                    errs => Err(errs),
                };

                (out, result)
            }
        }
    };

    implement
}

#[proc_macro_derive(Valid, attributes(valid))]
pub fn derive_valid(tokens: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(tokens as syn::DeriveInput);
    match do_derive_valid(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => proc_macro2::TokenStream::from(err).into(),
    }
}

fn do_derive_valid(derive: syn::DeriveInput) -> Result<proc_macro2::TokenStream, CompileError> {
    let span = derive.span();
    let fields = match derive.data {
        syn::Data::Struct(syn::DataStruct {
            fields: syn::Fields::Unnamed(FieldsUnnamed { unnamed, .. }),
            ..
        }) => unnamed,
        _ => {
            return Err(
                derime::span_compile_error!(span => "Only single unnamed structs are supported for now!"),
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
    let gate = gate.unwrap_or_else(|| syn::parse_quote!(()));

    let input = field.ty.clone();
    let path = match usage {
        Usage::Crate => quote! { crate },
        Usage::Lib => quote! { valigate },
    };
    let ident = derive.ident;

    Ok(quote! {
        impl #path::Valid for #ident {
            type In = #input;

            fn parse(input: Self::In) -> Result<Self, #path::Error> {
                #path::field_pipe(input, #gate).map(#ident)
            }
        }
    })
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
}
