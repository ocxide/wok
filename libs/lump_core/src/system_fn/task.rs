use std::marker::PhantomData;

use impls::{ParamBorrow, ParamOwned};

use crate::{
    param::Param,
    system::{IntoSystem, ProtoSystem, System, SystemIn, SystemInput},
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

pub trait SystemFn<Marker>: Sized + Send + Sync + 'static {
    type Input: SystemInput;
    type Params: Param;
    type Output;

    fn run(
        self,
        input: <Self::Input as SystemInput>::Inner<'_>,
        params: ParamBorrow<'_, Self::Params>,
    ) -> impl Future<Output = Self::Output> + Send;

    fn run_owned<'i>(
        self,
        input: <Self::Input as SystemInput>::Inner<'i>,
        params: ParamOwned<Self::Params>,
    ) -> impl Future<Output = Self::Output> + Send + 'i
    where
        Self::Input: Send + SystemInput<Inner<'i>: 'i>,
        Self::Output: Send + 'static;
}

impl<Marker, Func> System for FunctionSystem<Marker, Func>
where
    Marker: 'static,
    Func: SystemFn<Marker, Output: Send + 'static + Sync, Input: Send> + Clone,
{
    type In = Func::Input;
    type Out = Func::Output;

    fn init(&self, rw: &mut crate::world::access::SystemLock) {
        Func::Params::init(rw);
    }
}

impl<Marker, Func> ProtoSystem for FunctionSystem<Marker, Func>
where
    Marker: 'static,
    Func: SystemFn<Marker, Output: Send + 'static + Sync, Input: Send> + Clone,
{
    type Param = Func::Params;

    fn run<'i>(
        self,
        param: <Self::Param as Param>::Owned,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i {
        self.func.run_owned(input, param)
    }
}

#[doc(hidden)]
pub struct HasSystemInput;

#[doc(hidden)]
pub struct InputLessSystem;

mod impls {
    use super::{HasSystemInput, InputLessSystem, SystemFn};
    use crate::{param::Param, system::SystemInput};
    use async_fn_traits::{
        AsyncFn0, AsyncFn1, AsyncFn2, AsyncFn3, AsyncFn4, AsyncFn5, AsyncFn6, AsyncFn7, AsyncFn8,
    };
    use std::{future::Future, marker::PhantomData};

    pub type ParamBorrow<'p, T> = <T as Param>::AsRef<'p>;
    pub type ParamOwned<T> = <T as Param>::Owned;

    impl<Func, O> SystemFn<fn(O)> for Func
    where
        Func: AsyncFn0<Output = O, OutputFuture: Send> + Send + Sync + 'static,
    {
        type Input = ();
        type Params = ();
        type Output = O;

        fn run(
            self,
            _input: <Self::Input as crate::prelude::SystemInput>::Inner<'_>,
            _params: ParamBorrow<'_, Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send {
            self()
        }

        fn run_owned<'i>(
            self,
            input: <Self::Input as crate::prelude::SystemInput>::Inner<'i>,
            _params: super::ParamOwned<Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send + 'i
        where
            Self::Input: Send,
            Self::Output: Send + 'static,
        {
            self.run(input, ())
        }
    }

    impl<Func, I, O> SystemFn<(HasSystemInput, fn(I) -> O)> for Func
    where
        I: SystemInput + 'static,
        O: Send + 'static,
        Func: AsyncFn1<I, OutputFuture: Send, Output = O> + Send + Sync + 'static,
        Func: for<'i> AsyncFn1<I::Wrapped<'i>, OutputFuture: Send, Output = O>,
    {
        type Input = I;
        type Params = ();
        type Output = O;

        fn run_owned<'i>(
            self,
            input: <Self::Input as SystemInput>::Inner<'i>,
            _params: super::ParamOwned<Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send + 'i
        where
            Self::Input: Send,
            Self::Output: Send + 'static,
        {
            self.run(input, ())
        }

        fn run(
            self,
            input: <Self::Input as SystemInput>::Inner<'_>,
            _params: ParamBorrow<'_, Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send {
            fn call_inner<In: SystemInput, F, Out>(
                _: PhantomData<In>,
                f: F,
                input: In::Inner<'_>,
            ) -> impl Future<Output = Out> + Send
            where
                F: for<'i> AsyncFn1<In::Wrapped<'i>, OutputFuture: Send, Output = Out>,
            {
                f(In::wrap(input))
            }

            call_inner(PhantomData::<I>, self, input)
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
            _input: <Self::Input as SystemInput>::Inner<'_>,
            #[allow(non_snake_case, unused_parens)]
            ($($params),*): ParamBorrow<'_, Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send {
            self($($params),*)
        }

        #[allow(clippy::manual_async_fn, reason = "listening to clippy causes compile errors, screw you clippy")]
        fn run_owned<'i>(
            self,
            input: <Self::Input as SystemInput>::Inner<'i>,
            params: ParamOwned<Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send + 'i
        where
            Self::Input: Send + 'static,
        {
            async move {
                let params = Self::Params::from_owned(&params);
                self.run(input, params).await
            }
        }
    }
    };

    (HasSystemInput; $async_trait: ident; $($params:ident : $time: lifetime),* ) => {
    impl<Func, I, $($params),*, O> SystemFn<(HasSystemInput, fn(I, $(&'static $params),*, O))> for Func
    where
        I: SystemInput + 'static,
        $($params: Param),*,
        Func: Send + Sync + 'static + Clone,
        Func: $async_trait<I, $($params),* , Output = O>,
        Func: for<'i, $($time),*> $async_trait<I::Wrapped<'i>, $(ParamBorrow<$time, $params>),* , OutputFuture: Send, Output = O>,
    {
        type Input = I;
        #[allow(unused_parens)]
        type Params = ($($params),*);
        type Output = O;

        fn run(
            self,
            input: <Self::Input as SystemInput>::Inner<'_>,
            #[allow(non_snake_case, unused_parens)]
            ($($params),*): ParamBorrow<'_, Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send {
            self(I::wrap(input), $($params),*)
        }

        #[allow(clippy::manual_async_fn, reason = "listening to clippy causes compile errors, screw you clippy")]
        fn run_owned<'i>(
            self,
            input: <Self::Input as SystemInput>::Inner<'i>,
            params: ParamOwned<Self::Params>,
        ) -> impl Future<Output = Self::Output> + Send + 'i
        where
            Self::Input: Send + SystemInput<Inner<'i>: 'i>,
        {
            async move {
                let params = Self::Params::from_owned(&params);
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
