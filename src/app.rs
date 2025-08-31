use lump_core::{
    error::LumpUnknownError,
    prelude::{IntoBlockingSystem, IntoSystem, ProtoSystem, System, TaskSystem},
    world::{ConfigureWorld, World, WorldState},
};

use crate::{
    async_runtime::AsyncRuntime,
    locks_runtime::{LockingGateway, Runtime, SystemReserver},
    startup::Startup,
};

pub struct AppBuilder {
    world: World,
}

impl Default for AppBuilder {
    fn default() -> Self {
        let mut world = World::default();
        Startup::init(&mut world.center);

        Self { world }
    }
}

impl AppBuilder {
    pub fn build_parts(self) -> (Runtime, LockingGateway, WorldState) {
        let (state, center) = self.world.into_parts();

        let (rt, lockings) = Runtime::new(center);
        (rt, lockings, state)
    }

    pub fn build(self) -> App {
        let (rt, locking, state) = self.build_parts();

        App { rt, state, locking }
    }
}

impl ConfigureWorld for AppBuilder {
    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

pub struct App {
    state: WorldState,
    rt: Runtime,
    locking: LockingGateway,
}

impl App {
    pub async fn run<Marker, S: IntoAppRunnerSystem<Marker, Out = Result<(), LumpUnknownError>>>(
        mut self,
        runtime: impl AsyncRuntime,
        system: S,
    ) -> Result<(), LumpUnknownError> {
        Startup::create_invoker(&mut self.rt.world_center, &mut self.state, &runtime)
            .invoke()
            .await?;

        let system = system.into_runner_system();
        let systemid = self.rt.world_center.register_system(&system);

        let sys_fut = async {
            let permit = self
                .locking
                .clone()
                .with_state(&self.state)
                .lock(systemid)
                .await;

            let reserver = self.locking.with_state(&self.state);
            permit.run(&system, reserver).await
        };

        let bg_fut = async {
            self.rt.run().await;
            Ok(())
        };

        futures::future::try_join(sys_fut, bg_fut).await?;
        Ok(())
    }
}

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
    ) -> impl TaskSystem<In = SystemReserver<'static>, Out = Self::Out> + ProtoSystem
    {
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
    ) -> impl TaskSystem<In = SystemReserver<'static>, Out = Self::Out> + ProtoSystem
    {
        (|_: SystemReserver<'_>| {}).pipe_then(self).into_system()
    }
}

pub trait ConfigureMoreWorld: ConfigureWorld {
    fn add_plugin(self, plugin: impl crate::plugin::Plugin) -> Self;
}

impl ConfigureMoreWorld for AppBuilder {
    fn add_plugin(mut self, plugin: impl crate::plugin::Plugin) -> Self {
        plugin.setup(&mut self);
        self
    }
}

impl ConfigureWorld for &mut AppBuilder {
    fn world(&self) -> &World {
        AppBuilder::world(self)
    }

    fn world_mut(&mut self) -> &mut World {
        AppBuilder::world_mut(self)
    }
}

impl ConfigureMoreWorld for &mut AppBuilder {
    fn add_plugin(self, plugin: impl crate::plugin::Plugin) -> Self {
        plugin.setup(&mut *self);
        self
    }
}
