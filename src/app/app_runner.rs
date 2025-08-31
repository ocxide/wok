use lump_core::prelude::{IntoBlockingSystem, IntoSystem, ProtoSystem, System, TaskSystem};

use crate::prelude::SystemReserver;

pub trait IntoAppRunnerSystem<Marker> {
    type Out: Send + Sync + 'static;
    fn into_runner_system(
        self,
    ) -> impl TaskSystem<In = SystemReserver<'static>, Out = Self::Out> + ProtoSystem;
}

#[doc(hidden)]
pub struct WithInput;
impl<Marker, S> IntoAppRunnerSystem<(WithInput, Marker)> for S
where
    S: IntoSystem<Marker>,
    S::System: System<In = SystemReserver<'static>>,
{
    type Out = <S::System as System>::Out;
    fn into_runner_system(
        self,
    ) -> impl TaskSystem<In = SystemReserver<'static>, Out = Self::Out> + ProtoSystem {
        self.into_system()
    }
}

#[doc(hidden)]
pub struct WithoutInput;
impl<Marker, S> IntoAppRunnerSystem<(WithoutInput, Marker)> for S
where
    S: IntoSystem<Marker>,
    S::System: System<In = ()>,
{
    type Out = <S::System as System>::Out;
    fn into_runner_system(
        self,
    ) -> impl TaskSystem<In = SystemReserver<'static>, Out = Self::Out> + ProtoSystem {
        (|_: SystemReserver<'_>| {}).pipe_then(self).into_system()
    }
}

