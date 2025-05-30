use std::marker::PhantomData;

use crate::{
    dust::Dust, param::Param, system::{IntoSystem, StaticSystem, System, SystemFuture}
};

pub struct FunctionSystem<Marker, F> {
    func: F,
    _marker: PhantomData<fn(Marker)>,
}

impl<Marker, F> Clone for FunctionSystem<Marker, F>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            func: self.func.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Marker, F> Copy for FunctionSystem<Marker, F> where F: Copy {}

type ParamBorrow<'p, T> = <T as Param>::AsRef<'p>;
type ParamOwned<T> = <T as Param>::Owned;

pub trait SystemFn<Marker>: Sized + Send + Sync + 'static {
    type Input;
    type Params: Param;
    type Output;

    fn run(
        self,
        input: Self::Input,
        params: ParamBorrow<'_, Self::Params>,
    ) -> impl Future<Output = Self::Output> + Send;

    fn run_owned(
        self,
        input: Self::Input,
        params: ParamOwned<Self::Params>,
    ) -> impl Future<Output = Self::Output> + Send + 'static
    where
        Self::Input: Send + 'static,
        Self::Output: Send + 'static;
}

impl<Marker, Func> System for FunctionSystem<Marker, Func>
where
    Marker: 'static,
    Func: SystemFn<Marker, Output: Send + 'static + Sync, Input: Send> + Clone,
{
    type In = Func::Input;
    type Out = Func::Output;

    fn run(&self, dust: &Dust, input: Self::In) -> SystemFuture<Self> {
        let func = self.func.clone();
        let params = Func::Params::get(dust);

        let fut = func.run_owned(input, params);
        Box::pin(fut)
    }
}

impl<Marker, Func> StaticSystem for FunctionSystem<Marker, Func>
where
    Marker: 'static,
    Func: SystemFn<Marker, Output: Send + 'static + Sync, Input: Send> + Clone,
{
    type Params = ParamOwned<Func::Params>;

    fn get_params(dust: &Dust) -> Self::Params {
        Func::Params::get(dust)
    }

    fn run_static(
        &self,
        params: Self::Params,
        input: Self::In,
    ) -> impl Future<Output = Self::Out> + Send + 'static {
        self.func.clone().run_owned(input, params)
    }
}

pub trait IsSystemFn<Marker>: Sized + Send + Sync + 'static {
    type Input;
    type Params;
}

#[doc(hidden)]
pub struct HasSystemInput;

#[doc(hidden)]
pub struct InputLessSystem;

mod impls {
    use super::{HasSystemInput, InputLessSystem, SystemFn};
    use crate::param::{In, Param};
    use async_fn_traits::{
        AsyncFn0, AsyncFn1, AsyncFn2, AsyncFn3, AsyncFn4, AsyncFn5, AsyncFn6, AsyncFn7, AsyncFn8,
    };
    use std::future::Future;

    type ParamBorrow<'p, T> = <T as Param>::AsRef<'p>;
    type ParamOwned<T> = <T as Param>::Owned;

    impl<Func, O> SystemFn<fn(O)> for Func
    where
        Func: AsyncFn0<Output = O, OutputFuture: Send> + Send + Sync + 'static,
    {
        type Input = ();
        type Params = ();
        type Output = O;

        fn run(
            self,
            _: Self::Input,
            _: ParamBorrow<'_, Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send {
            self()
        }

        fn run_owned(
            self,
            _input: Self::Input,
            _params: ParamOwned<Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send + 'static
        where
            Self::Input: Send + 'static,
        {
            self()
        }
    }

    impl<Func, I> SystemFn<(HasSystemInput, fn(I))> for Func
    where
        Func: AsyncFn1<In<I>, OutputFuture: Send> + Send + Sync + 'static,
    {
        type Input = I;
        type Params = ();
        type Output = <Func as AsyncFn1<In<I>>>::Output;

        fn run(
            self,
            input: Self::Input,
            _: ParamBorrow<'_, Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send {
            self(In(input))
        }

        fn run_owned(
            self,
            input: Self::Input,
            _params: ParamOwned<Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send + 'static
        where
            Self::Input: Send + 'static,
            Self::Output: Send + 'static,
        {
            self.run(input, ())
        }
    }

    macro_rules! impl_system_fn {
        ($async_trait: ident; $($params:ident : $time: lifetime),* ) => {
        impl<Func, $($params),*, O> SystemFn<(InputLessSystem, fn($($params),*) -> O)> for Func
        where
            $($params: Param),*,
            Func: Send + Sync + 'static + Clone,
            Func: $async_trait<$($params),*, Output = O>,
            Func: for<$($time),*> $async_trait<$(ParamBorrow<$time, $params>),* , OutputFuture: Send, Output = O>,
        {
            type Input = ();
            #[allow(unused_parens)]
            type Params = ($($params),*);
            type Output = O;

            fn run(
                self,
                _input: Self::Input,
                #[allow(non_snake_case, unused_parens)]
                ($($params),*): ParamBorrow<'_, Self::Params>,
            ) -> impl Future<Output = Self::Output> + Send {
                self($($params),*)
            }

            #[allow(clippy::manual_async_fn, reason = "listening to clippy causes compile errors, screw you clippy")]
            fn run_owned(
                self,
                input: Self::Input,
                params: super::ParamOwned<Self::Params>,
            ) -> impl Future<Output = Self::Output> + Send + 'static
            where
                Self::Input: Send + 'static,
            {
                async move {
                    let params = Self::Params::as_ref(&params);
                    self.run(input, params).await
                }
            }
        }
        };

        (HasSystemInput; $async_trait: ident; $($params:ident : $time: lifetime),* ) => {
        impl<Func, I, $($params),*, O> SystemFn<(HasSystemInput, fn(I, $(&'static $params),*, O))> for Func
        where
            $($params: Param),*,
            Func: Send + Sync + 'static + Clone,
            Func: $async_trait<In<I>, $($params),* >,
            Func: for<$($time),*> $async_trait<In<I>, $(ParamBorrow<$time, $params>),* , OutputFuture: Send, Output = O>,
        {
            type Input = I;
            #[allow(unused_parens)]
            type Params = ($($params),*);
            type Output = O;

            fn run(
                self,
                input: Self::Input,
                #[allow(non_snake_case, unused_parens)]
                ($($params),*): ParamBorrow<'_, Self::Params>,
            ) -> impl Future<Output = Self::Output> + Send {
                self(In(input), $($params),*)
            }

            #[allow(clippy::manual_async_fn, reason = "listening to clippy causes compile errors, screw you clippy")]
            fn run_owned(
                self,
                input: Self::Input,
                params: super::ParamOwned<Self::Params>,
            ) -> impl Future<Output = Self::Output> + Send + 'static
            where
                Self::Input: Send + 'static,
            {
                async move {
                    let params = Self::Params::as_ref(&params);
                    self.run(input, params).await
                }
            }
        }
        };
    }

    // // AsyncFn0 is already implemented
    impl_system_fn!(AsyncFn1; P1: 'p1);
    impl_system_fn!(AsyncFn2; P1: 'p1, P2: 'p2);
    impl_system_fn!(AsyncFn3; P1: 'p1, P2: 'p2, P3: 'p3);
    impl_system_fn!(AsyncFn4; P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4);
    impl_system_fn!(AsyncFn5; P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5);
    impl_system_fn!(AsyncFn6; P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5, P6: 'p6);
    impl_system_fn!(AsyncFn7; P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5, P6: 'p6, P7: 'p7);
    impl_system_fn!(AsyncFn8; P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5, P6: 'p6, P7: 'p7, P8: 'p8);

    impl_system_fn!(HasSystemInput; AsyncFn2; P1: 'p1);
    impl_system_fn!(HasSystemInput; AsyncFn3; P1: 'p1, P2: 'p2);
    impl_system_fn!(HasSystemInput; AsyncFn4; P1: 'p1, P2: 'p2, P3: 'p3);
    impl_system_fn!(HasSystemInput; AsyncFn5; P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4);
    impl_system_fn!(HasSystemInput; AsyncFn6; P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5);
    impl_system_fn!(HasSystemInput; AsyncFn7; P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5, P6: 'p6);
    impl_system_fn!(HasSystemInput; AsyncFn8; P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5, P6: 'p6, P7: 'p7);
}

impl<Marker, Func> IntoSystem<Marker> for Func
where
    Marker: 'static,
    Func: SystemFn<Marker, Input: Send, Output: Send + Sync + 'static> + Clone,
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
