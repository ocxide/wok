use std::marker::PhantomData;

use crate::{
    Dust,
    param::Param,
    system::{IntoSystem, System},
};

use std::future::Future;

pub type ParamBorrow<'p, T> = <T as Param>::AsRef<'p>;

pub struct FunctionSystem<Marker, F> {
    func: F,
    _marker: PhantomData<fn(Marker)>,
}

impl<Func, P1, O> System for FunctionSystem<fn(&'static P1, O), Func>
where
    P1: Param,
    O: Send + Sync + 'static,
    Func: Send + Sync + 'static + Copy,
    Func: async_fn_traits::AsyncFn1<P1>,
    Func: for<'p> async_fn_traits::AsyncFn1<ParamBorrow<'p, P1>, OutputFuture: Send, Output = O>,
{
    type Input = ();
    type Output = O;

    fn run(
        &self,
        dust: &Dust,
        _input: Self::Input,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let params = P1::get(dust);
        let func = self.func;

        async move {
            let params = P1::as_ref(&params);
            func(params).await
        }
    }
}

impl<Marker, Func> IntoSystem<Marker> for Func
where
    FunctionSystem<Marker, Func>: System,
{
    type System = FunctionSystem<Marker, Func>;

    #[inline]
    fn into_system(self) -> Self::System {
        FunctionSystem {
            func: self,
            _marker: PhantomData,
        }
    }
}
