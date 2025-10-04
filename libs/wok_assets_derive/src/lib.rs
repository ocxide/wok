use derime::CompileError;
use syn::spanned::Spanned;

#[proc_macro_derive(AssetsCollection)]
pub fn param_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match do_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => proc_macro2::token_stream::TokenStream::from(err).into(),
    }
}

fn do_derive(ast: syn::DeriveInput) -> Result<proc_macro2::TokenStream, CompileError> {
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let thing_name = &ast.ident;

    let fields = match ast.data {
        syn::Data::Struct(syn::DataStruct {
            fields: syn::Fields::Named(syn::FieldsNamed { named, .. }),
            ..
        }) => named,
        _ => {
            return Err(derime::span_compile_error!(
                ast.span() =>
                "AssetsCollection can only be derived for structs with named fields",
            ));
        }
    };

    let types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();

    let fields = fields
        .iter()
        .map(|field| field.ident.as_ref().unwrap())
        .collect::<Vec<_>>();

    let tokens = quote::quote! {
        impl #impl_generics wok_assets::AssetsCollection for #thing_name #ty_generics #where_clause {
            type Assets = (#(wok::prelude::ResInitMarker<#types>),*);

            fn insert_all(self, commands: &mut wok::prelude::Commands) {
                let Self { #(#fields,)* } = self;
                #(commands.insert_resource(#fields);)*
            }
        }
    };

    Ok(tokens)
}

#[test]
fn works() {
    let result = do_derive(syn::parse_quote! {
        struct Foo {
            a: u32,
            b: u32,
        }
    })
    .unwrap();

    assert_eq!(
        result.to_string(),
        quote::quote! {
            impl wok_assets::AssetsCollection for Foo {
                type Assets = (wok::prelude::ResInitMarker<u32>, wok::prelude::ResInitMarker<u32>);

                fn insert_all(self, commands: &mut wok::prelude::Commands) {
                    let Self { a, b, } = self;
                    commands.insert_resource(a);
                    commands.insert_resource(b);
                }
            }
        }.to_string()
    );
}
