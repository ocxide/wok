use as_surreal_bind::do_as_surreal_bind_derive;

#[proc_macro_derive(AsSurrealBind, attributes(surreal_bind))]
pub fn as_surreal_bind_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match do_as_surreal_bind_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => proc_macro2::TokenStream::from(err).into(),
    }
}

#[proc_macro_derive(FromSurrealBind, attributes(surreal_bind))]
pub fn from_surreal_bind_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match from_surreal_bind::from_surreal_db_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => proc_macro2::TokenStream::from(err).into(),
    }
}

mod as_surreal_bind;
mod from_surreal_bind;
