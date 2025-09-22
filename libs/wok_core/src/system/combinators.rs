use futures::{FutureExt, future::Either};

use crate::param::Param;

use super::{
    IntoSystem, ProtoTaskSystem, System, SystemIn, SystemInput,
    blocking::{IntoBlockingSystem, ProtoBlockingSystem},
};

pub struct IntoTryThenSystem<S1, S2> {
    pub system1: S1,
    pub system2: S2,
}

impl<S1, S1Marker, S2, S2Marker, Ok, Err> IntoSystem<(S1Marker, S2Marker, fn(Ok) -> Err)>
    for IntoTryThenSystem<S1, S2>
where
    S1: IntoBlockingSystem<S1Marker>,
    S2: IntoSystem<S2Marker>,
    S1::System: System<Out = Result<Ok, Err>>,
    <S2::System as System>::In: for<'i> SystemInput<Inner<'i> = Ok>,
    Ok: Send + Sync + 'static,
    Err: Send + Sync + 'static,
{
    type System = TryThenSystem<S1::System, S2::System, Ok, Err>;

    fn into_system(self) -> Self::System {
        TryThenSystem {
            system1: self.system1.into_system(),
            system2: self.system2.into_system(),
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct TryThenSystem<S1, S2, Ok, Err> {
    system1: S1,
    system2: S2,
    _marker: std::marker::PhantomData<fn(Ok) -> Err>,
}

impl<S1, S2, Ok, Err> Clone for TryThenSystem<S1, S2, Ok, Err>
where
    S1: Clone,
    S2: Clone,
{
    fn clone(&self) -> Self {
        Self {
            system1: self.system1.clone(),
            system2: self.system2.clone(),
            _marker: self._marker,
        }
    }
}

impl<S1, S2, Ok, Err> System for TryThenSystem<S1, S2, Ok, Err>
where
    Ok: Send + Sync + 'static,
    Err: Send + Sync + 'static,
    S1: ProtoBlockingSystem<Out = Result<Ok, Err>>,
    S2: ProtoTaskSystem,
    S2::In: for<'i> SystemInput<Inner<'i> = Ok>,
{
    type In = S1::In;
    type Out = Result<S2::Out, Err>;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        self.system1.init(rw);
        self.system2.init(rw);
    }
}

impl<S1, S2, Ok, Err> ProtoTaskSystem for TryThenSystem<S1, S2, Ok, Err>
where
    Ok: Send + Sync + 'static,
    Err: Send + Sync + 'static,
    S1: ProtoBlockingSystem<Out = Result<Ok, Err>>,
    S2: ProtoTaskSystem,
    S2::In: for<'i> SystemInput<Inner<'i> = Ok>,
{
    type Param = (S1::Param, S2::Param);

    fn run<'i>(
        self,
        (mut param1, param2): <Self::Param as Param>::Owned,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i {
        let result = self.system1.run(S1::Param::from_owned(&mut param1), input);

        let input2 = match result {
            Ok(ok) => ok,
            Err(err) => return Either::Left(std::future::ready(Err(err))),
        };

        Either::Right(self.system2.run(param2, input2).map(|out| Ok(out)))
    }
}

#[derive(Clone)]
pub struct PipeThenSystem<S1, S2> {
    system1: S1,
    system2: S2,
}

impl<S1, S2> System for PipeThenSystem<S1, S2>
where
    S1: ProtoBlockingSystem,
    S2: ProtoTaskSystem,
    S2::In: for<'i> SystemInput<Inner<'i> = S1::Out>,
{
    type In = S1::In;
    type Out = S2::Out;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        self.system1.init(rw);
        self.system2.init(rw);
    }
}

impl<S1, S2> ProtoTaskSystem for PipeThenSystem<S1, S2>
where
    S1: ProtoBlockingSystem,
    S2: ProtoTaskSystem,
    S2::In: for<'i> SystemInput<Inner<'i> = S1::Out>,
{
    type Param = (S1::Param, S2::Param);

    fn run<'i>(
        self,
        (mut param1, param2): <Self::Param as Param>::Owned,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i {
        let out = self.system1.run(S1::Param::from_owned(&mut param1), input);
        self.system2.run(param2, out)
    }
}

pub struct IntoPipeThenSystem<S1, S2> {
    pub system1: S1,
    pub system2: S2,
}

pub struct IsIntoPipeThen;

impl<S1, S1Marker, S2, S2Marker> IntoSystem<(S1Marker, S2Marker, IsIntoPipeThen)>
    for IntoPipeThenSystem<S1, S2>
where
    S1: IntoBlockingSystem<S1Marker>,
    S2: IntoSystem<S2Marker>,
    <S2::System as System>::In: for<'i> SystemInput<Inner<'i> = <S1::System as System>::Out>,
{
    type System = PipeThenSystem<S1::System, S2::System>;

    fn into_system(self) -> Self::System {
        PipeThenSystem {
            system1: self.system1.into_system(),
            system2: self.system2.into_system(),
        }
    }
}

#[derive(Clone)]
pub struct MapSystem<S1, S2> {
    system1: S1,
    system2: S2,
}

impl<S1, S2> System for MapSystem<S1, S2>
where
    S1: ProtoTaskSystem,
    S2: ProtoBlockingSystem,
    S2::In: for<'i> SystemInput<Inner<'i> = S1::Out>,
{
    type In = S1::In;
    type Out = S2::Out;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        self.system1.init(rw);
        self.system2.init(rw);
    }
}

impl<S1, S2> ProtoTaskSystem for MapSystem<S1, S2>
where
    S1: ProtoTaskSystem,
    S2: ProtoBlockingSystem,
    S2::In: for<'i> SystemInput<Inner<'i> = S1::Out>,
{
    type Param = (S1::Param, S2::Param);

    fn run<'i>(
        self,
        (param1, mut param2): <Self::Param as Param>::Owned,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i {
        self.system1
            .run(param1, input)
            .map(move |out| self.system2.run(S2::Param::from_owned(&mut param2), out))
    }
}

pub struct IntoMapSystem<S1, S2> {
    pub system1: S1,
    pub system2: S2,
}

pub struct IsIntoMap;

impl<S1, S1Marker, S2, S2Marker> IntoSystem<(S1Marker, S2Marker, IsIntoMap)>
    for IntoMapSystem<S1, S2>
where
    S1: IntoSystem<S1Marker>,
    S2: IntoBlockingSystem<S2Marker>,
    <S2::System as System>::In: for<'i> SystemInput<Inner<'i> = <S1::System as System>::Out>,
{
    type System = MapSystem<S1::System, S2::System>;

    fn into_system(self) -> Self::System {
        MapSystem {
            system1: self.system1.into_system(),
            system2: self.system2.into_system(),
        }
    }
}

pub struct IntoPipeBlockingSystem<S1, S2> {
    pub system1: S1,
    pub system2: S2,
}

impl<S1, Marker1, S2, Marker2> IntoBlockingSystem<(Marker1, Marker2, IsIntoMap)> for IntoPipeBlockingSystem<S1, S2>
where
    S1: IntoBlockingSystem<Marker1>,
    S2: IntoBlockingSystem<Marker2>,
    <S2::System as System>::In: for<'i> SystemInput<Inner<'i> = <S1::System as System>::Out>,
{
    type System = PipeBlockingSystem<S1::System, S2::System>;

    fn into_system(self) -> Self::System {
        PipeBlockingSystem {
            system1: self.system1.into_system(),
            system2: self.system2.into_system(),
        }
    }
}

#[derive(Clone)]
pub struct PipeBlockingSystem<S1, S2> {
    system1: S1,
    system2: S2,
}

impl<S1, S2> System for PipeBlockingSystem<S1, S2>
where
    S1: System,
    S2: System,
    S2::In: for<'i> SystemInput<Inner<'i> = S1::Out>,
{
    type In = S1::In;
    type Out = S2::Out;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        self.system1.init(rw);
        self.system2.init(rw);
    }
}

impl<S1, S2> ProtoBlockingSystem for PipeBlockingSystem<S1, S2>
where
    S1: ProtoBlockingSystem,
    S2: ProtoBlockingSystem,
    S2::In: for<'i> SystemInput<Inner<'i> = S1::Out>,
{
    type Param = (S1::Param, S2::Param);

    fn run(
        &self,
        param: <Self::Param as Param>::AsRef<'_>,
        input: SystemIn<'_, Self>,
    ) -> Self::Out {
        let out = self.system1.run(param.0, input);
        self.system2.run(param.1, out)
    }
}
