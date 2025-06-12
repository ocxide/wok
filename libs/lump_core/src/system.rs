use std::pin::Pin;

use crate::world::{WorldState, access::SystemLock};

pub type ScopedFut<'i, Out> = Pin<Box<dyn Future<Output = Out> + Send + 'i>>;
pub type SystemFuture<'i, S> = Pin<Box<dyn Future<Output = <S as System>::Out> + Send + 'i>>;
pub type DynSystem<In, Out> = Box<dyn TaskSystem<In = In, Out = Out> + Send + Sync + 'static>;

pub trait System: Send + Sync + 'static {
    type In;
    type Out: Send + Sync + 'static;

    fn init(&self, rw: &mut SystemLock);
}

// Dyn compatible
pub trait TaskSystem: System {
    fn run<'i>(&self, world: &WorldState, input: Self::In) -> SystemFuture<'i, Self>
    where
        Self::In: 'i;
}

pub trait StaticSystem: System {
    type Params;
    fn get_params(world: &WorldState) -> Self::Params;
    fn run_static(
        &self,
        params: Self::Params,
        input: Self::In,
    ) -> impl Future<Output = Self::Out> + Send + 'static;
}

pub trait BoundSystem<In, Out: Send + Sync + 'static> {
    fn run<'i>(&self, world: &WorldState, input: In) -> ScopedFut<'i, Out>
    where
        In: 'i;
}

impl<TS, In, Out: Send + Sync + 'static> BoundSystem<In, Out> for TS
where
    TS: TaskSystem<In = In, Out = Out>,
{
    #[inline]
    fn run<'i>(&self, world: &WorldState, input: In) -> ScopedFut<'i, Out>
    where
        In: 'i,
    {
        <Self as TaskSystem>::run(self, world, input)
    }
}

pub trait IntoSystem<Marker> {
    type System: System + TaskSystem + StaticSystem;

    fn into_system(self) -> Self::System;
    // fn pipe<S2, MarkerS2>(self, s2: S2) -> IntoSystemPipe<Self::System, S2::System>
    // where
    //     S2: IntoSystem<MarkerS2>,
    //     S2::System: StaticSystem<In = <Self::System as System>::Out>,
    //     Self: Sized,
    // {
    //     IntoSystemPipe::new(self.into_system(), s2.into_system())
    // }
}

// mod combinator {
//     use crate::world::WorldState;
//
//     use super::{IntoSystem, StaticSystem, System, SystemFuture, TaskSystem};
//
//     pub struct IntoSystemPipe<S1, S2> {
//         s1: S1,
//         s2: S2,
//     }
//
//     impl<S1, S2> IntoSystemPipe<S1, S2> {
//         pub const fn new(s1: S1, s2: S2) -> Self {
//             Self { s1, s2 }
//         }
//     }
//
//     #[doc(hidden)]
//     pub struct SystemPipeMarker;
//
//     impl<S1, S2> IntoSystem<SystemPipeMarker> for IntoSystemPipe<S1, S2>
//     where
//         S1: StaticSystem<In: Send, Out: Send, Params: Send>,
//         S2: StaticSystem<In = S1::Out, Params: Send> + Clone,
//     {
//         type System = SystemPipe<S1, S2>;
//         fn into_system(self) -> Self::System {
//             SystemPipe {
//                 s1: self.s1,
//                 s2: self.s2,
//             }
//         }
//     }
//
//     pub struct SystemPipe<S1: StaticSystem, S2: StaticSystem> {
//         s1: S1,
//         s2: S2,
//     }
//
//     impl<S1, S2> System for SystemPipe<S1, S2>
//     where
//         S1: StaticSystem<In: Send, Out: Send, Params: Send>,
//         S2: StaticSystem<In = S1::Out, Params: Send> + Clone,
//     {
//         type In = S1::In;
//         type Out = S2::Out;
//
//         fn init(&self, rw: &mut crate::world::access::SystemLock) {
//             self.s1.init(rw);
//             self.s2.init(rw);
//         }
//     }
//
//     impl<S1, S2> StaticSystem for SystemPipe<S1, S2>
//     where
//         S1: StaticSystem<In: Send, Out: Send, Params: Send>,
//         S2: StaticSystem<In = S1::Out, Params: Send> + Clone,
//     {
//         type Params = (S1::Params, S2::Params);
//
//         fn get_params(world: &WorldState) -> Self::Params {
//             (S1::get_params(world), S2::get_params(world))
//         }
//
//         fn run_static(
//             &self,
//             (p1, p2): Self::Params,
//             input: Self::In,
//         ) -> impl Future<Output = Self::Out> + Send + 'static {
//             let fut1 = self.s1.run_static(p1, input);
//             let s2 = self.s2.clone();
//
//             async move {
//                 let out1 = fut1.await;
//                 s2.run_static(p2, out1).await
//             }
//         }
//     }
//
//     impl<S1, S2> TaskSystem for SystemPipe<S1, S2>
//     where
//         S1: StaticSystem<In: Send, Out: Send, Params: Send>,
//         S2: StaticSystem<In = S1::Out, Params: Send> + Clone,
//     {
//         fn run(&self, world: &WorldState, input: Self::In) -> SystemFuture<Self> {
//             Box::pin(self.run_static((S1::get_params(world), S2::get_params(world)), input))
//         }
//     }
// }
