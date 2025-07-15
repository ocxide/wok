use std::pin::Pin;

use crate::{
    param::Param,
    world::{WorldState, access::SystemLock},
};

use combinators::IntoMapSystem;
pub use input::*;

mod combinators;
mod input;

pub type ScopedFut<'i, Out> = Pin<Box<dyn Future<Output = Out> + Send + 'i>>;
pub type SystemFuture<'i, S> = Pin<Box<dyn Future<Output = <S as System>::Out> + Send + 'i>>;
pub type DynSystem<In, Out> = Box<dyn TaskSystem<In = In, Out = Out> + Send + Sync + 'static>;
pub type SystemIn<'i, S> = <<S as System>::In as SystemInput>::Inner<'i>;

pub trait System: Send + Sync + 'static {
    type In: SystemInput;
    type Out: Send + Sync + 'static;

    fn init(&self, rw: &mut SystemLock);
}

// Dyn compatible
pub trait TaskSystem: System {
    fn run<'i>(&self, world: &WorldState, input: SystemIn<'i, Self>) -> SystemFuture<'i, Self>
    where
        Self::In: 'i;

    fn create_task(&self, world: &WorldState) -> SystemTask<Self::In, Self::Out>;
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

pub trait ProtoTask<'p, Input: SystemInput + 'static, Out: Send + Sync + 'static>:
    Send + 'static
{
    fn run<'i>(self, input: Input::Inner<'i>) -> impl Future<Output = Out> + Send + 'i;
    fn into_task(self) -> crate::prelude::SystemTask<Input, Out>
    where
        Self: Sized,
    {
        crate::prelude::SystemTask::new(|input, _| Box::pin(self.run(input)))
    }
}

// Allow zero-cost abstraction
pub trait ProtoSystem: System {
    type Param: Param;

    fn run<'i>(
        &self,
        param: <Self::Param as Param>::AsRef<'i>,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i;

    fn run_owned<'i>(
        &self,
        param: <Self::Param as Param>::Owned,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i;

    fn create_task_owned(
        &self,
        param: <Self::Param as Param>::Owned,
    ) -> impl ProtoTask<'static, Self::In, Self::Out>;
}

pub trait IntoSystem<Marker> {
    type System: System + TaskSystem + ProtoSystem;

    fn into_system(self) -> Self::System;
    fn map<F, Out>(self, func: F) -> IntoMapSystem<F, Self>
    where
        Self: Sized,
        F: Fn(<Self::System as System>::Out) -> Out + Clone,
    {
        IntoMapSystem { func, system: self }
    }
}

impl<S: ProtoSystem> TaskSystem for S {
    fn run<'i>(&self, world: &WorldState, input: SystemIn<'i, Self>) -> SystemFuture<'i, Self>
    where
        Self::In: 'i,
    {
        let param = S::Param::get(world);
        let fut = self.run_owned(param, input);
        Box::pin(fut)
    }

    fn create_task(&self, world: &WorldState) -> SystemTask<Self::In, Self::Out> {
        let param = S::Param::get(world);
        let task = self.create_task_owned(param);

        task.into_task()
    }
}
