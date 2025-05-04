use std::marker::PhantomData;

use crate::system::{IntoSystem, System};

pub struct FunctionSystem<Marker, F> {
    func: F,
    _marker: PhantomData<fn(Marker)>,
}

pub trait IsSystemFn<Marker>: Send + Sync + 'static {
    type Input;
    type Params;
    type Output;
}

mod impls {
    use super::{FunctionSystem, IsSystemFn};
    use crate::{
        Dust,
        param::{In, Param},
        system::System,
    };
    use async_fn_traits::{
        AsyncFn0, AsyncFn1, AsyncFn2, AsyncFn3, AsyncFn4, AsyncFn5, AsyncFn6, AsyncFn7, AsyncFn8,
    };
    use std::future::Future;

    type ParamBorrow<'p, T> = <T as Param>::AsRef<'p>;

    #[doc(hidden)]
    pub struct HasSystemInput;

    impl<Func, O> System for FunctionSystem<fn(O), Func>
    where
        O: Send + Sync + 'static,
        Func: Send + Sync + 'static + Copy,
        Func: AsyncFn0<OutputFuture: Send, Output = O>,
    {
        type Input = ();
        type Output = O;

        fn run(
            &self,
            _dust: &Dust,
            _input: Self::Input,
        ) -> impl Future<Output = Self::Output> + Send + 'static {
            let func = self.func;

            async move { func().await }
        }
    }

    impl<Func, O> IsSystemFn<fn(O)> for Func
    where
        Func: AsyncFn0 + Send + Sync + 'static,
    {
        type Input = ();
        type Params = ();
        type Output = O;
    }

    impl<Func, I, O> System for FunctionSystem<(HasSystemInput, fn(I, O)), Func>
    where
        O: Send + Sync + 'static,
        I: Send + Sync + 'static,
        Func: Send + Sync + 'static + Copy,
        Func: AsyncFn1<In<I>, OutputFuture: Send, Output = O>,
    {
        type Input = I;
        type Output = O;

        fn run(
            &self,
            _dust: &Dust,
            input: Self::Input,
        ) -> impl Future<Output = Self::Output> + Send + 'static {
            let func = self.func;

            async move { func(In(input)).await }
        }
    }

    impl<Func, I, O> IsSystemFn<(HasSystemInput, fn(I, O))> for Func
    where
        Func: AsyncFn1<In<I>> + Send + Sync + 'static,
    {
        type Input = I;
        type Params = ();
        type Output = O;
    }

    macro_rules! impl_system_fn {
    ($async_trait: ident; $($params:ident : $time: lifetime),* ) => {
        impl<Func, $($params),*, O> System for FunctionSystem<fn($(&'static $params),*, O), Func>
        where
            $($params: Param),*,
            O: Send + Sync + 'static,
            Func: Send + Sync + 'static + Copy,
            Func: $async_trait<$($params),* >,
            Func: for<$($time),*> $async_trait<$(ParamBorrow<$time, $params>),* , OutputFuture: Send, Output = O>,
        {
            type Input = ();
            type Output = O;

            fn run(
                &self,
                dust: &Dust,
                _input: Self::Input,
            ) -> impl Future<Output = Self::Output> + Send + 'static {
                $(
                #[allow(non_snake_case)]
                let $params = $params::get(dust)
                );*;
                let func = self.func;

                async move {
                    $(
                    #[allow(non_snake_case)]
                    let $params = $params::as_ref(&$params);
                    )*
                    func(
                        $($params),*
                    ).await
                }
            }
        }

        impl<Func, $($params),*, O> IsSystemFn<fn($(&'static $params),*, O)> for Func
        where
            $($params: Param),*,
            Func: $async_trait<$($params),*, Output = O> + Send + Sync + 'static,
        {
            type Input = ();
            #[allow(unused_parens)]
            type Params = ($($params),*);
            type Output = O;
        }
    };

    ($marker: ident; $async_trait: ident; $($params:ident : $time: lifetime),* ) => {
        impl<Func, I, $($params),*, O> System for FunctionSystem<(HasSystemInput, fn(I, $(&'static $params),*), O), Func>
        where
            I: Send + Sync + 'static,
            $($params: Param),*,
            O: Send + Sync + 'static,
            Func: Send + Sync + 'static + Copy,
            Func: $async_trait<In<I>, $($params),* >,
            Func: for<$($time),*> $async_trait<In<I>, $(ParamBorrow<$time, $params>),* , OutputFuture: Send, Output = O>,
        {
            type Input = I;
            type Output = O;

            fn run(
                &self,
                dust: &Dust,
                input: Self::Input,
            ) -> impl Future<Output = Self::Output> + Send + 'static {
                $(
                #[allow(non_snake_case)]
                let $params = $params::get(dust)
                );*;
                let func = self.func;

                async move {
                    $(
                    #[allow(non_snake_case)]
                    let $params = $params::as_ref(&$params);
                    )*
                    func(
                        In(input),
                        $($params),*
                    ).await
                }
            }
        }

        impl<Func, I, $($params),*, O> IsSystemFn<(HasSystemInput, fn(I, $(&'static $params),*), O)> for Func
        where
            Func: $async_trait<In<I>, $($params),*, Output = O> + Send + Sync + 'static,
        {
            type Input = I;
            #[allow(unused_parens)]
            type Params = ($($params),*);
            type Output = O;
        }
    };
}

    // AsyncFn0 is already implemented
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
    Func: IsSystemFn<Marker>,
    FunctionSystem<Marker, Func>: System<Input = Func::Input, Output = Func::Output>,
{
    type In = Func::Input;
    type Out = Func::Output;
    type System = FunctionSystem<Marker, Func>;

    #[inline]
    fn into_system(self) -> Self::System {
        FunctionSystem {
            func: self,
            _marker: PhantomData,
        }
    }
}
