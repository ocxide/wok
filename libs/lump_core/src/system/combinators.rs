use futures::FutureExt;

use super::{IntoSystem, ProtoSystem, ProtoTask, System, SystemIn, SystemInput};

pub struct InputAdapterTask<T, F, Marker> {
    task: T,
    func: F,
    _marker: std::marker::PhantomData<fn(Marker)>,
}

impl<T, F, Input, Ok1, Ok2, Err> ProtoTask<'static, Input, Result<Ok2, Err>>
    for InputAdapterTask<T, F, fn(Ok1, Input, Err)>
where
    Ok2: Send + Sync + 'static,
    Err: Send + Sync + 'static,
    Ok1: SystemInput + 'static,
    Input: SystemInput + 'static,
    T: ProtoTask<'static, Ok1, Result<Ok2, Err>>,
    F: for<'i> Fn(Input::Inner<'i>, &'i [(); 0]) -> Result<Ok1::Inner<'i>, Err> + Send + 'static,
{
    fn run<'i>(
        self,
        input: <Input as SystemInput>::Inner<'i>,
    ) -> impl Future<Output = Result<Ok2, Err>> + Send + 'i {
        let result = (self.func)(input, &[]);
        let adapted = match result {
            Ok(input) => input,
            Err(err) => return futures::future::Either::Left(async move { Err(err) }),
        };

        let fut = self.task.run(adapted);
        futures::future::Either::Right(fut)
    }
}

pub struct InputAdapterSystem<F, S, Marker> {
    func: F,
    system: S,
    _marker: std::marker::PhantomData<fn(Marker)>,
}

impl<F, S, Input, Ok, Err> System for InputAdapterSystem<F, S, (Input, Err)>
where
    Input: SystemInput + 'static,
    Ok: Send + Sync + 'static,
    Err: Send + Sync + 'static,
    F: Send
        + Sync
        + 'static
        + for<'i> Fn(Input::Inner<'i>, &'i [(); 0]) -> Result<SystemIn<'i, S>, Err>,
    S: System<Out = Result<Ok, Err>>,
{
    type In = Input;
    type Out = S::Out;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        self.system.init(rw)
    }
}

impl<F, S, Input, Ok, Err> ProtoSystem for InputAdapterSystem<F, S, (Input, Err)>
where
    Input: SystemInput + 'static,
    Ok: Send + Sync + 'static,
    Err: Send + Sync + 'static,
    F: Send
        + Sync
        + 'static
        + for<'i> Fn(Input::Inner<'i>, &'i [(); 0]) -> Result<SystemIn<'i, S>, Err>
        + Clone,
    S: ProtoSystem<Out = Result<Ok, Err>>,
{
    type Param = S::Param;
    fn run<'i>(
        &self,
        param: <Self::Param as crate::prelude::Param>::AsRef<'i>,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i {
        let result = (self.func)(input, &[]);
        let adapted = match result {
            Ok(input) => input,
            Err(err) => return futures::future::Either::Left(async move { Err(err) }),
        };

        let fut = self.system.run(param, adapted);
        futures::future::Either::Right(fut)
    }

    fn run_owned<'i>(
        &self,
        param: <Self::Param as crate::prelude::Param>::Owned,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i {
        let result = (self.func)(input, &[]);
        let adapted = match result {
            Ok(input) => input,
            Err(err) => return futures::future::Either::Left(async move { Err(err) }),
        };

        let fut = self.system.run_owned(param, adapted);
        futures::future::Either::Right(fut)
    }

    fn create_task_owned(
        &self,
        param: <Self::Param as crate::prelude::Param>::Owned,
    ) -> impl super::ProtoTask<'static, Self::In, Self::Out> {
        let task = self.system.create_task_owned(param);
        InputAdapterTask {
            task,
            func: self.func.clone(),
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct IntoMapSystem<F, S> {
    pub(crate) func: F,
    pub(crate) system: S,
}

pub type IntoMapSystemMarker<Out, Marker> = (Out, Marker);

impl<F, S, Out, Marker> IntoSystem<IntoMapSystemMarker<Out, Marker>> for IntoMapSystem<F, S>
where
    Out: Send + Sync + 'static,
    F: Send + Sync + 'static + Clone + Fn(<S::System as System>::Out) -> Out,
    S: IntoSystem<Marker>,
{
    type System = MapSystem<F, S::System, Out>;

    fn into_system(self) -> Self::System {
        MapSystem {
            func: self.func,
            system: self.system.into_system(),
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct MapSystemTask<F, T, Marker> {
    task: T,
    func: F,
    _marker: std::marker::PhantomData<fn(Marker)>,
}

impl<F, T, Input, Out1, Out2> ProtoTask<'static, Input, Out2>
    for MapSystemTask<F, T, (Input, Out1, Out2)>
where
    Input: SystemInput + 'static,
    Out1: Send + Sync + 'static,
    Out2: Send + Sync + 'static,
    F: Send + Sync + 'static + Clone + Fn(Out1) -> Out2,
    T: ProtoTask<'static, Input, Out1>,
{
    fn run<'i>(
        self,
        input: <Input as SystemInput>::Inner<'i>,
    ) -> impl Future<Output = Out2> + Send + 'i {
        let fut = self.task.run(input);
        fut.map(self.func)
    }
}

pub struct MapSystem<F, S, Marker> {
    func: F,
    system: S,
    _marker: std::marker::PhantomData<fn(Marker)>,
}

impl<F, S, Out> System for MapSystem<F, S, Out>
where
    Out: Send + Sync + 'static,
    F: Send + Sync + 'static + Fn(S::Out) -> Out,
    S: System,
{
    type In = S::In;
    type Out = Out;

    fn init(&self, rw: &mut crate::world::SystemLock) {
        self.system.init(rw)
    }
}

impl<F, S, Out> ProtoSystem for MapSystem<F, S, Out>
where
    Out: Send + Sync + 'static,
    F: Send + Sync + 'static + Clone + Fn(S::Out) -> Out,
    S: ProtoSystem,
{
    type Param = S::Param;
    fn run<'i>(
        &self,
        param: <Self::Param as crate::prelude::Param>::AsRef<'i>,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i {
        let fut = self.system.run(param, input);
        let func = self.func.clone();

        fut.map(func)
    }

    fn run_owned<'i>(
        &self,
        param: <Self::Param as crate::prelude::Param>::Owned,
        input: SystemIn<'i, Self>,
    ) -> impl Future<Output = Self::Out> + Send + 'i {
        let fut = self.system.run_owned(param, input);
        let func = self.func.clone();

        fut.map(func)
    }

    fn create_task_owned(
        &self,
        param: <Self::Param as crate::prelude::Param>::Owned,
    ) -> impl ProtoTask<'static, Self::In, Self::Out> {
        let task = self.system.create_task_owned(param);
        MapSystemTask {
            task,
            func: self.func.clone(),
            _marker: std::marker::PhantomData,
        }
    }
}
