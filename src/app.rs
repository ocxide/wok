mod app_runner;

use app_runner::IntoAppRunnerSystem;
use lump_core::{
    error::LumpUnknownError, runtime::RuntimeAddon, world::{ConfigureWorld, World, WorldState}
};

use crate::{
    async_executor::AsyncExecutor,
    runtime::{LockingGateway, Runtime, RuntimeCfg},
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
    pub async fn run<Marker, S, AsyncRt: AsyncExecutor, RtAddon: RuntimeAddon>(
        mut self,
        cfg: RuntimeCfg<AsyncRt, RtAddon>,
        system: S,
    ) -> Result<(), LumpUnknownError>
    where
        S: IntoAppRunnerSystem<Marker, Out = Result<(), LumpUnknownError>>,
    {
        Startup::create_invoker(&mut self.rt.world_center, &mut self.state, &cfg.async_runtime)
            .invoke()
            .await?;
        let addon = RtAddon::create(&mut self.state);

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
            self.rt.run(&self.state, addon).await;
            Ok(())
        };

        futures::future::try_join(sys_fut, bg_fut).await?;
        Ok(())
    }
}

pub trait ConfigureApp: ConfigureWorld {
    fn add_plugin(self, plugin: impl crate::plugin::Plugin) -> Self;
}

impl ConfigureApp for AppBuilder {
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

impl ConfigureApp for &mut AppBuilder {
    fn add_plugin(self, plugin: impl crate::plugin::Plugin) -> Self {
        plugin.setup(&mut *self);
        self
    }
}
