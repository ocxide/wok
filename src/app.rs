mod app_runner;

use app_runner::IntoAppRunnerSystem;
use lump_core::{
    async_executor::AsyncExecutor,
    error::LumpUnknownError,
    runtime::RuntimeAddon,
    world::{ConfigureWorld, UnsafeWorldState, World},
};

use crate::{
    runtime::{Runtime, RuntimeCfg},
    startup::Startup,
};

pub struct App {
    world: World,
}

impl Default for App {
    fn default() -> Self {
        let mut world = World::default();
        Startup::init(&mut world.center);

        Self { world }
    }
}

impl App {
    pub async fn run<Marker, S, AsyncRt: AsyncExecutor, RtAddon: RuntimeAddon>(
        self,
        cfg: RuntimeCfg<AsyncRt, RtAddon>,
        system: S,
    ) -> Result<(), LumpUnknownError>
    where
        S: IntoAppRunnerSystem<Marker, Out = Result<(), LumpUnknownError>>,
    {
        let mut state = self.world.state;
        let mut center = self.world.center;

        // Run addon build before startup to allow the use of ParamsClient
        let addon = RtAddon::create(&mut state);

        Startup::create_invoker(&mut center, &mut state, &cfg.async_runtime)
            .invoke()
            .await?;

        let state = UnsafeWorldState::new(state);

        let system = system.into_runner_system();
        let systemid = center.register_system(&system);

        let (runtime, locking) = Runtime::new(&state, &mut center.system_locks, addon);

        let sys_fut = async {
            let permit = locking.clone().with_state(&state).lock(systemid).await;

            let reserver = locking.with_state(&state);
            permit.run(&system, reserver).await
        };

        let bg_fut = async {
            runtime.run(&cfg.async_runtime).await;
            Ok(())
        };

        futures::future::try_join(sys_fut, bg_fut).await?;
        Ok(())
    }
}

impl ConfigureWorld for App {
    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

pub trait ConfigureApp: ConfigureWorld {
    fn add_plugin(self, plugin: impl crate::plugin::Plugin) -> Self;
}

impl ConfigureApp for App {
    fn add_plugin(mut self, plugin: impl crate::plugin::Plugin) -> Self {
        plugin.setup(&mut self);
        self
    }
}

impl ConfigureWorld for &mut App {
    fn world(&self) -> &World {
        App::world(self)
    }

    fn world_mut(&mut self) -> &mut World {
        App::world_mut(self)
    }
}

impl ConfigureApp for &mut App {
    fn add_plugin(self, plugin: impl crate::plugin::Plugin) -> Self {
        plugin.setup(&mut *self);
        self
    }
}
