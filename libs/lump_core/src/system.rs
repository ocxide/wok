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
    use crate::{param::Param, world::WorldState};

    use super::{IntoSystem, System, SystemIn, SystemInput, combinators::IntoTryThenSystem};

    pub trait BlockingSystem: System {
        fn run(&self, world: &WorldState, input: SystemIn<'_, Self>) -> Self::Out;
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
        fn run(&self, world: &WorldState, input: SystemIn<'_, Self>) -> Self::Out {
            let param = S::Param::get(world);
            self.run(S::Param::as_ref(&param), input)
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
    }
}
