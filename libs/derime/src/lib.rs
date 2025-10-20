#[derive(Debug)]
pub struct CompileError(pub syn::Error);

impl From<CompileError> for proc_macro2::TokenStream {
    fn from(err: CompileError) -> Self {
        err.0.into_compile_error()
    }
}

#[macro_export]
macro_rules! span_compile_error(
    ($span: expr => $msg: expr) => {
        CompileError(syn::Error::new($span, $msg))
    };

    ($span: expr => $msg: expr, $( $param: expr ),*) => {
        CompileError(syn::Error::new($span, format!($msg, $( $param ),*)))
    }
);

pub use attr::*;

mod attr;
