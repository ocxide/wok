use std::marker::PhantomData;

use crate::{
    Dust,
    param::Param,
    system::{IntoSystem, System},
};

pub trait SystemFn<Marker>: Send + Sync + 'static {
    type Input;
    type Params: Param;
    type Output: Send + Sync + 'static;

    fn run(
        &self,
        input: Self::Input,
        params: Self::Params,
    ) -> impl Future<Output = Self::Output> + Send + 'static;
}

impl<Func, P1, O, Fut> SystemFn<fn(O, P1) -> Fut> for Func
where
    O: Send + Sync + 'static,
    P1: Param,
    Fut: Future<Output = O> + Send + Sync + 'static,
    Self: Fn(P1) -> Fut + Send + 'static,
    Self: Send + Sync + Copy + 'static,
{
    type Input = ();
    type Params = P1;
    type Output = O;

    #[inline]
    fn run(
        &self,
        _input: Self::Input,
        params: Self::Params,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        self(params)
    }
}

pub struct FunctionSystem<Marker, F> {
    func: F,
    _marker: PhantomData<fn(Marker)>,
}

impl<Marker, F> System for FunctionSystem<Marker, F>
where
    F: SystemFn<Marker> + Copy + 'static + Sized + 'static,
    Marker: 'static,
    F::Input: Send + Sync + 'static,
{
    type Input = F::Input;
    type Output = F::Output;

    fn run(
        &self,
        dust: &Dust,
        input: Self::Input,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let params = F::Params::get(dust);
        let func = self.func;

        async move { func.run(input, params).await }
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
