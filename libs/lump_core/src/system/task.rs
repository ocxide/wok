use std::pin::Pin;

use crate::{param::Param, world::UnsafeWorldState};

use super::{IntoBlockingSystem, System, SystemIn, SystemInput, combinators::IntoMapSystem};

pub type ScopedFut<'i, Out> = Pin<Box<dyn Future<Output = Out> + Send + 'i>>;
pub type SystemFuture<'i, S> = Pin<Box<dyn Future<Output = <S as System>::Out> + Send + 'i>>;
pub type DynSystem<In, Out> = Box<dyn TaskSystem<In = In, Out = Out> + Send + Sync + 'static>;

// Dyn compatible
pub trait TaskSystem: System {
    /// # Safety
    /// The caller must ensure no dupliated mutable access is happening
    unsafe fn run_owned<'i>(
        self,
        state: &UnsafeWorldState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self>;

    /// # Safety
    /// The caller must ensure no dupliated mutable access is happening
    unsafe fn run<'i>(
        &self,
        state: &UnsafeWorldState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self>;

    /// # Safety
    /// The caller must ensure no dupliated mutable access is happening
    unsafe fn create_task(&self, state: &UnsafeWorldState) -> SystemTask<Self::In, Self::Out>;
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
    fn map<S2, Marker2>(self, system: S2) -> IntoMapSystem<Self, S2>
    where
        Self: Sized,
        S2: IntoBlockingSystem<Marker2>,
        <S2::System as System>::In: for<'i> SystemInput<Inner<'i> = <Self::System as System>::Out>,
    {
        IntoMapSystem {
            system1: self,
            system2: system,
        }
    }
}

impl<S: ProtoSystem> TaskSystem for S {
    unsafe fn run_owned<'i>(
        self,
        state: &UnsafeWorldState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self> {
        let param = unsafe { S::Param::get(state) };
        Box::pin(self.run(param, input))
    }

    unsafe fn run<'i>(
        &self,
        state: &UnsafeWorldState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self> {
        let system = self.clone();
        unsafe { system.run_owned(state, input) }
    }

    unsafe fn create_task(&self, state: &UnsafeWorldState) -> SystemTask<Self::In, Self::Out> {
        let system = self.clone();
        let param = unsafe { S::Param::get(state) };
        SystemTask::new(|input, _| Box::pin(system.run(param, input)))
    }
}

impl<In: SystemInput + 'static, Out: Send + Sync + 'static> System for DynSystem<In, Out> {
    type In = In;
    type Out = Out;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        self.as_ref().init(rw);
    }
}

impl<In: SystemInput + 'static, Out: Send + Sync + 'static> TaskSystem for DynSystem<In, Out> {
    unsafe fn run<'i>(
        &self,
        state: &UnsafeWorldState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self> {
        unsafe { self.as_ref().run(state, input) }
    }

    unsafe fn run_owned<'i>(
        self,
        state: &UnsafeWorldState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self> {
        unsafe { self.as_ref().run(state, input) }
    }

    unsafe fn create_task(&self, state: &UnsafeWorldState) -> SystemTask<Self::In, Self::Out> {
        unsafe { self.as_ref().create_task(state) }
    }
}
