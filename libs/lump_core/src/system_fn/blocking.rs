use std::marker::PhantomData;

use crate::system::System;
use crate::system::blocking::{IntoBlockingSystem, ProtoBlockingSystem};
use crate::{param::Param, system::SystemInput};

pub struct FunctionSystem<Fn, Marker> {
    func: Fn,
    _marker: PhantomData<fn(Marker)>,
}

impl<Fn, Marker> Clone for FunctionSystem<Fn, Marker>
where
    Fn: Clone,
{
    fn clone(&self) -> Self {
        Self {
            func: self.func.clone(),
            _marker: PhantomData,
        }
    }
}

pub trait SystemFn<Marker>: Send + Sync + 'static {
    type Params: Param;
    type Input: SystemInput;
    type Output: Send + Sync + 'static;

    fn run(
        &self,
        input: <Self::Input as SystemInput>::Inner<'_>,
        params: <Self::Params as Param>::AsRef<'_>,
    ) -> Self::Output;
}

pub type ParamBorrow<'p, T> = <T as Param>::AsRef<'p>;
struct HasSystemInput;

macro_rules! impl_system_fn {
    ($($params:ident : $time: lifetime),*) => {
    impl<Func, $($params),*, O> SystemFn<fn($($params),*) -> O> for Func
        where
            $($params: Param),*,
            O: Send + Sync + 'static,
            Func: Send + Sync + 'static + Clone,
            Func: Fn($($params),*) -> O,
            Func: for<$($time),*> Fn($(ParamBorrow<$time, $params>),*) -> O
    {
        type Input = ();
        #[allow(unused_parens)]
        type Params = ($($params),*);
        type Output = O;

        fn run(
            &self,
            _input: <Self::Input as SystemInput>::Inner<'_>,
            #[allow(non_snake_case, unused_parens)]
            ($($params),*): ParamBorrow<'_, Self::Params>,
        ) -> Self::Output {
            self($($params),*)
        }
    }

    impl<Func, I, $($params),*, O> SystemFn<(HasSystemInput, fn(I, $($params),*) -> O)> for Func
    where
        I: SystemInput + 'static,
        $($params: Param),*,
        O: Send + Sync + 'static,
        Func: Send + Sync + 'static + Clone,
        Func: Fn(I, $($params),*) -> O,
        Func: for<'i, $($time),*> Fn(I::Wrapped<'i>, $(ParamBorrow<$time, $params>),*) -> O
    {
        type Input = I;
        #[allow(unused_parens)]
        type Params = ($($params),*);
        type Output = O;

        fn run(
            &self,
            input: <Self::Input as SystemInput>::Inner<'_>,
            #[allow(non_snake_case, unused_parens)]
            ($($params),*): ParamBorrow<'_, Self::Params>,
        ) -> Self::Output {
            self(I::wrap(input), $($params),*)
        }
    }
    };
}

impl<Func, O> SystemFn<fn() -> O> for Func
where
    O: Send + Sync + 'static,
    Func: Send + Sync + 'static + Clone,
    Func: Fn() -> O,
{
    type Input = ();
    #[allow(unused_parens)]
    type Params = ();
    type Output = O;
    fn run(
        &self,
        _input: <Self::Input as SystemInput>::Inner<'_>,
        #[allow(non_snake_case, unused_parens)] (): ParamBorrow<'_, Self::Params>,
    ) -> Self::Output {
        self()
    }
}

impl<Func, I, O> SystemFn<(HasSystemInput, fn(I) -> O)> for Func
where
    I: SystemInput + 'static,
    O: Send + Sync + 'static,
    Func: Send + Sync + 'static + Clone,
    Func: Fn(I) -> O,
    Func: for<'i> Fn(I::Wrapped<'i>) -> O,
{
    type Input = I;
    #[allow(unused_parens)]
    type Params = ();
    type Output = O;
    fn run(
        &self,
        input: <Self::Input as SystemInput>::Inner<'_>,
        #[allow(non_snake_case, unused_parens)] (): ParamBorrow<'_, Self::Params>,
    ) -> Self::Output {
        self(I::wrap(input))
    }
}
impl_system_fn!(P1: 'p1);
impl_system_fn!(P1: 'p1, P2: 'p2);
impl_system_fn!(P1: 'p1, P2: 'p2, P3: 'p3);
impl_system_fn!(P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4);
impl_system_fn!(P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5);
impl_system_fn!(P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5, P6: 'p6);
impl_system_fn!(P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5, P6: 'p6, P7: 'p7);
impl_system_fn!(P1: 'p1, P2: 'p2, P3: 'p3, P4: 'p4, P5: 'p5, P6: 'p6, P7: 'p7, P8: 'p8);

impl<Func, Marker: 'static> System for FunctionSystem<Func, Marker>
where
    Func: SystemFn<Marker>,
{
    type In = Func::Input;
    type Out = Func::Output;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        Func::Params::init(rw);
    }
}

impl<Func, Marker: 'static> ProtoBlockingSystem for FunctionSystem<Func, Marker>
where
    Func: SystemFn<Marker> + Clone,
{
    type Param = Func::Params;
    fn run(
        &self,
        param: <Self::Param as Param>::AsRef<'_>,
        input: crate::prelude::SystemIn<'_, Self>,
    ) -> Self::Out {
        self.func.run(input, param)
    }
}

impl<Func, Marker: 'static> IntoBlockingSystem<Marker> for Func
where
    Func: SystemFn<Marker> + Clone,
{
    type System = FunctionSystem<Func, Marker>;

    #[inline]
    fn into_system(self) -> Self::System {
        FunctionSystem {
            func: self,
            _marker: PhantomData,
        }
    }
}
