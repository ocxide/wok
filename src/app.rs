use lump_core::{
    error::LumpUnknownError,
    prelude::{IntoSystem, System},
    world::{ConfigureWorld, World, WorldState},
};

use crate::{
    async_runtime::AsyncRuntime,
    locks_runtime::{Runtime, SystemLocking},
    startup::Startup,
};

pub struct AppBuilder {
    world: World,
}

impl Default for AppBuilder {
    fn default() -> Self {}
}

impl AppBuilder {
    pub fn build_parts(self) -> (Runtime, SystemLocking, WorldState) {
        let (state, center) = self.world.into_parts();

        let (rt, lockings) = Runtime::new(center);
        (rt, lockings, state)
    }

    pub fn build(self) -> App {
        let (rt, locking, mut state) = self.build_parts;

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
    locking: SystemLocking,
}

impl App {
    pub async fn run<S, Marker>(
        mut self,
        runtime: impl AsyncRuntime,
        system: S,
    ) -> Result<(), LumpUnknownError>
    where
        S: IntoSystem<Marker>,
        S::System: System<In = (), Out = Result<(), LumpUnknownError>>,
    {
        Startup::create_invoker(&mut self.rt.world_center, &mut self.state, &runtime)
            .invoke()
            .await?;

        let system = system.into_system();
        let systemid = self.rt.world_center.register_system(&system);

        let sys_fut = self
            .locking
            .clone()
            .with_state(&self.state)
            .lock(systemid)
            .await
            .run(&system, ());

        let bg_fut = async {
            self.rt.run().await;
            Ok(())
        };

        futures::future::try_join(sys_fut, bg_fut).await?;

        Ok(())
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
