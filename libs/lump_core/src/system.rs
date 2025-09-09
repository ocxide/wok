pub use input::*;

mod combinators;
mod input;

mod task;

pub use task::*;

use crate::world::SystemLock;

pub type SystemIn<'i, S> = <<S as System>::In as SystemInput>::Inner<'i>;

pub trait System: Send + Sync + 'static {
    type In: SystemInput;
    type Out: Send + Sync + 'static;

    fn init(&self, rw: &mut SystemLock);
}

pub use blocking::*;

pub mod blocking {
    use crate::{param::Param, world::UnsafeWorldState};

    use super::{
        IntoSystem, System, SystemIn, SystemInput,
        combinators::{IntoPipeThenSystem, IntoTryThenSystem},
    };

    pub trait BlockingSystem: System {
        unsafe fn run(&self, state: &UnsafeWorldState, input: SystemIn<'_, Self>) -> Self::Out;
    }

    pub trait ProtoBlockingSystem: System + Clone {
        type Param: Param;
        fn run(
            &self,
            param: <Self::Param as Param>::AsRef<'_>,
            input: SystemIn<'_, Self>,
        ) -> Self::Out;
    }

    impl<S: ProtoBlockingSystem> BlockingSystem for S {
        unsafe fn run(&self, state: &UnsafeWorldState, input: SystemIn<'_, Self>) -> Self::Out {
            let param = unsafe { S::Param::get_ref(state) };
            self.run(param, input)
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
    }
}
