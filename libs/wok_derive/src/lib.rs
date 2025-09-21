mod derime;

#[proc_macro_derive(Param, attributes(param))]
pub fn param_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    match param_derive::do_param_derive(ast) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into(),
    }
}

mod param_derive;

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

macro_rules! span_compile_error(
    ($span: expr => $msg: expr) => {
        crate::CompileError(quote::quote_spanned! { $span => compile_error!($msg) }.into())
    };

    ($span: expr => $msg: expr, $( $param: expr ),*) => {
        crate::CompileError(quote::quote_spanned! { $span => compile_error!($msg, $( $param ),*) }.into())
    }
);

pub(crate) use span_compile_error;
