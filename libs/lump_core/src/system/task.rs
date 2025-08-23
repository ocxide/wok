use std::pin::Pin;

use crate::{param::Param, world::WorldState};

use super::{System, SystemIn, SystemInput};

pub type ScopedFut<'i, Out> = Pin<Box<dyn Future<Output = Out> + Send + 'i>>;
pub type SystemFuture<'i, S> = Pin<Box<dyn Future<Output = <S as System>::Out> + Send + 'i>>;
pub type DynSystem<In, Out> = Box<dyn TaskSystem<In = In, Out = Out> + Send + Sync + 'static>;

// Dyn compatible
pub trait TaskSystem: System {
    fn run<'i>(&self, world: &WorldState, input: SystemIn<'i, Self>) -> SystemFuture<'i, Self>;

    fn create_task(&self, world: &WorldState) -> SystemTask<Self::In, Self::Out>;

    fn run_owned<'i>(self, world: &WorldState, input: SystemIn<'i, Self>)
    -> SystemFuture<'i, Self>;
}

pub struct SystemTask<In: SystemInput + 'static, Out: Send + Sync + 'static>(
    #[allow(
        clippy::type_complexity,
        reason = "I am obsuring the type behind type `SystemTask`"
    )]
    Box<dyn for<'i> FnOnce(In::Inner<'i>, &'i [(); 0]) -> ScopedFut<'i, Out> + Send + 'static>,
);

impl<In: SystemInput + 'static, Out: Send + Sync + 'static> SystemTask<In, Out> {
    pub fn run<'i>(self, input: In::Inner<'i>) -> ScopedFut<'i, Out> {
        self.0(input, &[])
    }

    pub fn new<F>(f: F) -> Self
    where
        F: for<'i> FnOnce(In::Inner<'i>, &'i [(); 0]) -> ScopedFut<'i, Out> + 'static + Send,
    {
        Self(Box::new(f))
    }
}

// Allow zero-cost abstraction
pub trait ProtoSystem: System + Clone {
    type Param: Param;

    fn run<'i>(
        self,
        param: <Self::Param as Param>::Owned,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i;
}

pub trait IntoSystem<Marker> {
    type System: System + TaskSystem + ProtoSystem;

    fn into_system(self) -> Self::System;
}

impl<S: ProtoSystem> TaskSystem for S {
    fn run<'i>(&self, world: &WorldState, input: SystemIn<'i, Self>) -> SystemFuture<'i, Self> {
        self.clone().run_owned(world, input)
    }

    fn run_owned<'i>(
        self,
        world: &WorldState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self> {
        let param = S::Param::get(world);
        Box::pin(self.run(param, input))
    }

    fn create_task(&self, world: &WorldState) -> SystemTask<Self::In, Self::Out> {
        let system = self.clone();
        let param = S::Param::get(world);
        SystemTask::new(|input, _| Box::pin(system.run(param, input)))
    }
}
