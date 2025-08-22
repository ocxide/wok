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

pub mod blocking {
    use crate::{param::Param, world::WorldState};

    use super::{System, SystemIn};

    pub trait BlockingSystem: System {
        fn run(&self, world: &WorldState, input: SystemIn<'_, Self>) -> Self::Out;
    }

    pub trait ProtoBlockingSystem: System {
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
        type System: BlockingSystem;

        fn into_system(self) -> Self::System;
    }
}
