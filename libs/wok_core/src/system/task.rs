use std::pin::Pin;

use crate::{
    param::{BorrowMutParam, Param},
    world::{UnsafeMutState, UnsafeWorldState},
};

use super::{
    IntoBlockingSystem, ProtoSystem, System, SystemIn, SystemInput, combinators::IntoMapSystem,
};

pub type ScopedFut<'i, Out> = Pin<Box<dyn Future<Output = Out> + Send + 'i>>;
pub type SystemFuture<'i, S> = Pin<Box<dyn Future<Output = <S as System>::Out> + Send + 'i>>;
pub type DynTaskSystem<In, Out> = Box<dyn BorrowTaskSystem<In = In, Out = Out> + Send + Sync>;

// Dyn compatible
/// # Safety
/// Only impl if the params are `BorrowMutParam`
pub unsafe trait BorrowTaskSystem: TaskSystem {
    /// # Safety
    /// The caller must ensure no dupliated mutable access is happening
    unsafe fn run<'i>(
        &self,
        state: &UnsafeWorldState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self> {
        unsafe { <Self as TaskSystem>::owned_run(self, state.as_unsafe_mut(), input) }
    }

    /// # Safety
    /// The caller must ensure no dupliated mutable access is happening
    unsafe fn create_task(&self, state: &UnsafeWorldState) -> SystemTask<Self::In, Self::Out> {
        unsafe { <Self as TaskSystem>::owned_create_task(self, state.as_unsafe_mut()) }
    }
}

pub trait TaskSystem: System {
    /// # Safety
    /// The caller must ensure no dupliated mutable access is happening
    unsafe fn owned_run<'i>(
        &self,
        state: &UnsafeMutState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self>;

    /// # Safety
    /// The caller must ensure no dupliated mutable access is happening
    unsafe fn owned_create_task(&self, state: &UnsafeMutState) -> SystemTask<Self::In, Self::Out>;
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
pub trait ProtoTaskSystem: ProtoSystem + Clone {
    fn run<'i>(
        self,
        param: <Self::Param as Param>::Owned,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i;
}

pub trait IntoSystem<Marker> {
    type System: System + ProtoSystem + TaskSystem + ProtoTaskSystem;

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

impl<S: ProtoTaskSystem> TaskSystem for S {
    unsafe fn owned_run<'i>(
        &self,
        state: &UnsafeMutState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self> {
        let param = unsafe { S::Param::get_owned(state) };
        let system = self.clone();

        Box::pin(system.run(param, input))
    }

    unsafe fn owned_create_task(&self, state: &UnsafeMutState) -> SystemTask<Self::In, Self::Out> {
        let system = self.clone();
        let param = unsafe { S::Param::get_owned(state) };
        SystemTask::new(|input, _| Box::pin(system.run(param, input)))
    }
}

unsafe impl<S: ProtoTaskSystem + TaskSystem> BorrowTaskSystem for S where S::Param: BorrowMutParam {}

impl<In: SystemInput + 'static, Out: Send + Sync + 'static> System for DynTaskSystem<In, Out> {
    type In = In;
    type Out = Out;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        self.as_ref().init(rw);
    }
}

impl<In: SystemInput + 'static, Out: Send + Sync + 'static> TaskSystem for DynTaskSystem<In, Out> {
    unsafe fn owned_run<'i>(
        &self,
        state: &UnsafeMutState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self> {
        unsafe { TaskSystem::owned_run(self.as_ref(), state, input) }
    }

    unsafe fn owned_create_task(&self, state: &UnsafeMutState) -> SystemTask<Self::In, Self::Out> {
        unsafe { TaskSystem::owned_create_task(self.as_ref(), state) }
    }
}

// Its ok since DynTaskSystem already implements dyn BorrowTaskSystem
unsafe impl<In: SystemInput + 'static, Out: Send + Sync + 'static> BorrowTaskSystem
    for DynTaskSystem<In, Out>
{
}

impl<In: SystemInput + 'static, Out: Send + Sync + 'static> System
    for Box<dyn TaskSystem<In = In, Out = Out> + Send + Sync>
{
    type In = In;
    type Out = Out;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        self.as_ref().init(rw);
    }
}

impl<In: SystemInput + 'static, Out: Send + Sync + 'static> TaskSystem
    for Box<dyn TaskSystem<In = In, Out = Out> + Send + Sync>
{
    unsafe fn owned_run<'i>(
        &self,
        state: &UnsafeMutState,
        input: SystemIn<'i, Self>,
    ) -> SystemFuture<'i, Self> {
        unsafe { TaskSystem::owned_run(self.as_ref(), state, input) }
    }

    unsafe fn owned_create_task(&self, state: &UnsafeMutState) -> SystemTask<Self::In, Self::Out> {
        unsafe { TaskSystem::owned_create_task(self.as_ref(), state) }
    }
}
