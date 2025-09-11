use futures::FutureExt;
use lump_core::{
    async_executor::AsyncExecutor,
    error::LumpUnknownError,
    prelude::{IntoSystem, System},
    runtime::RuntimeAddon,
    world::gateway::WorldMut,
    world::{ConfigureWorld, World},
};

use crate::{
    runtime::{RuntimeBuilder, RuntimeCfg},
    startup::Startup,
};

pub struct App {
    world: World,
}

impl Default for App {
    fn default() -> Self {
        let mut world = World::default();
        Startup::init(&mut world);

        Self { world }
    }
}

impl App {
    pub async fn run<Marker, AsyncRt: AsyncExecutor, RtAddon: RuntimeAddon>(
        self,
        cfg: RuntimeCfg<AsyncRt, RtAddon>,
        system: impl IntoSystem<Marker, System: System<In = (), Out = Result<(), LumpUnknownError>>>,
    ) -> Result<(), LumpUnknownError> {
        let mut state = self.world.state;
        let mut center = self.world.center;

        // Run addon build before startup to allow the use of ParamsClient
        let addon = RtAddon::create(&mut state);
        let (runtime, gateway) = RuntimeBuilder::new(addon);

        state.resources.insert(gateway.downgrade());

        Startup::create_invoker(&mut center, &mut state, &cfg.async_runtime)
            .invoke()
            .await?;

        let state = state.wrap();
        let system = system.into_system();
        let system = center.register_system(system);

        let mut world_mut = WorldMut::new(&state, &mut center.system_locks);
        let sys_fut = match world_mut.local_tasks().run(system.entry_ref(), ()) {
            Ok(fut) => fut.map(|(id, out)| out.map(|ok| (id, ok))),
            Err(_) => return Ok(()),
        };

        let sys_fut = sys_fut.map(move |out| {
            // Keep alive the gateway until the main system is done
            let _ = gateway;
            out
        });

        let mut runtime = runtime.build(&state, &mut center.system_locks);

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
