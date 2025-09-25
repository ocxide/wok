pub use input::*;

mod combinators;
mod input;

mod task;

pub use task::*;

use crate::{param::Param, world::SystemLock};

pub type SystemIn<'i, S> = <<S as System>::In as SystemInput>::Inner<'i>;

pub trait System: Send + Sync + 'static {
    type In: SystemInput;
    type Out: Send + Sync + 'static;

    fn init(&self, rw: &mut SystemLock);
}

pub trait ProtoSystem: System {
    type Param: Param;
}

pub use blocking::*;

pub mod blocking {
    use crate::{
        param::{BorrowMutParam, Param},
        world::UnsafeWorldState,
    };

    use super::{
        IntoSystem, System, SystemIn, SystemInput,
        combinators::{IntoPipeBlockingSystem, IntoPipeThenSystem, IntoTryThenSystem},
    };

    pub type DynBlockingSystem<In, Out> = Box<dyn BlockingSystem<In = In, Out = Out> + Send + Sync>;

    pub struct BlockingCaller<In: SystemInput + 'static, Out>(
        #[allow(clippy::type_complexity)] Box<dyn for<'i> FnOnce(In::Inner<'i>) -> Out + Send>,
    );

    impl<In: SystemInput + 'static, Out> BlockingCaller<In, Out> {
        pub fn run(self, input: In::Inner<'_>) -> Out {
            (self.0)(input)
        }
    }

    pub trait BlockingSystem: System {
        /// # Safety
        /// The caller must ensure no duplicated mutable access is happening
        unsafe fn run(&self, state: &UnsafeWorldState, input: SystemIn<'_, Self>) -> Self::Out;

        /// # Safety
        /// The caller must ensure no duplicated mutable access is happening
        unsafe fn create_caller(
            &self,
            state: &UnsafeWorldState,
        ) -> BlockingCaller<Self::In, Self::Out>;
    }

    pub trait ProtoBlockingSystem: System + Clone {
        type Param: BorrowMutParam;
        fn run(
            &self,
            param: <Self::Param as Param>::AsRef<'_>,
            input: SystemIn<'_, Self>,
        ) -> Self::Out;
    }

    impl<S: ProtoBlockingSystem> BlockingSystem for S {
        unsafe fn run(&self, state: &UnsafeWorldState, input: SystemIn<'_, Self>) -> Self::Out {
            let param = unsafe { S::Param::borrow(state) };
            self.run(param, input)
        }

        unsafe fn create_caller(
            &self,
            state: &UnsafeWorldState,
        ) -> BlockingCaller<Self::In, Self::Out> {
            let mut param = unsafe { S::Param::borrow_owned(state) };
            let this = self.clone();
            BlockingCaller(Box::new(move |input| {
                this.run(S::Param::from_owned(&mut param), input)
            }))
        }
    }

    pub trait IntoBlockingSystem<Marker> {
        type System: ProtoBlockingSystem + BlockingSystem;

        fn into_system(self) -> Self::System;
        fn try_then<S2, S2Marker, Ok, Err>(self, system2: S2) -> IntoTryThenSystem<Self, S2>
        where
            Ok: Send + Sync + 'static,
            Err: Send + Sync + 'static,
            Self: Sized,
            Self::System: System<Out = Result<Ok, Err>>,
            S2: IntoSystem<S2Marker>,
            <S2::System as System>::In: for<'i> SystemInput<Inner<'i> = Ok>,
        {
            IntoTryThenSystem {
                system1: self,
                system2,
            }
        }

        fn pipe_then<S2, S2Marker>(self, system2: S2) -> IntoPipeThenSystem<Self, S2>
        where
            Self: Sized,
            Self::System: System,
            S2: IntoSystem<S2Marker>,
            <S2::System as System>::In:
                for<'i> SystemInput<Inner<'i> = <Self::System as System>::Out>,
        {
            IntoPipeThenSystem {
                system1: self,
                system2,
            }
        }

        fn pipe<S2, S2Marker>(self, system2: S2) -> IntoPipeBlockingSystem<Self, S2>
        where
            Self: Sized,
            Self::System: System,
            S2: IntoBlockingSystem<S2Marker>,
            <S2::System as System>::In:
                for<'i> SystemInput<Inner<'i> = <Self::System as System>::Out>,
        {
            IntoPipeBlockingSystem {
                system1: self,
                system2,
            }
        }
    }

    impl<In: SystemInput + 'static, Out: Send + Sync + 'static> System for DynBlockingSystem<In, Out> {
        type In = In;
        type Out = Out;

        fn init(&self, rw: &mut crate::world::SystemLock) {
            self.as_ref().init(rw);
        }
    }

    impl<In: SystemInput + 'static, Out: Send + Sync + 'static> BlockingSystem
        for DynBlockingSystem<In, Out>
    {
        unsafe fn run(&self, state: &UnsafeWorldState, input: SystemIn<'_, Self>) -> Self::Out {
            unsafe { self.as_ref().run(state, input) }
        }

        unsafe fn create_caller(
            &self,
            state: &UnsafeWorldState,
        ) -> BlockingCaller<Self::In, Self::Out> {
            unsafe { self.as_ref().create_caller(state) }
        }
    }
}
