use futures::FutureExt;
use wok_core::{
    async_executor::AsyncExecutor,
    error::WokUnknownError,
    prelude::{IntoBlockingSystem, IntoSystem, System, TaskSystem},
    runtime::RuntimeAddon,
    world::{ConfigureWorld, UnsafeMutState, World, WorldCenter, gateway::SystemEntry},
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
        system: impl AppSystem<Marker>,
    ) -> Result<(), WokUnknownError> {
        let mut state = self.world.state;
        let mut center = self.world.center;

        // Run addon build before startup to allow the use of ParamsClient
        let (addon, rests) = RtAddon::create(&mut state);
        let (runtime, gateway) = RuntimeBuilder::new(&mut state, addon);

        Startup::create_invoker(&mut center, &mut state, &cfg.async_runtime)
            .invoke()
            .await?;

        debug_assert!(center.system_locks.is_all_free(), "All resources must be free after startup");

        let state = state.wrap();

        // Safety: we are the only owner
        let sys_fut = unsafe { system.app_run(state.as_unsafe_mut(), &mut center) };
        let sys_fut = sys_fut.map(move |out| {
            // Keep alive the gateway until the main system is done
            let _ = gateway;
            let _ = rests;
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

pub trait AppSystem<Marker> {
    /// # Safety
    /// Caller must ensure it is the only owner
    unsafe fn app_run(
        self,
        state: &UnsafeMutState,
        center: &mut WorldCenter,
    ) -> impl Future<Output = Result<(), WokUnknownError>> + 'static;
}

#[doc(hidden)]
pub struct TaskAppSystem;

impl<Marker, S> AppSystem<(Marker, TaskAppSystem)> for S
where
    S: IntoSystem<Marker>,
    S::System: System<In = (), Out = Result<(), WokUnknownError>>,
{
    unsafe fn app_run(
        self,
        state: &UnsafeMutState,
        center: &mut WorldCenter,
    ) -> impl Future<Output = Result<(), WokUnknownError>> + 'static {
        let system = center.register_system(self.into_system());
        let mut world = unsafe { state.borrow_world_mut(&mut center.system_locks) };

        let fut = world
            .local_tasks()
            .run(system.entry_ref(), ())
            .expect("to run main app system");

        fut.map(|(_, out)| out)
    }
}

#[doc(hidden)]
pub struct InlineAppSystem;

impl<Marker, S, SChoice> AppSystem<(Marker, SChoice, InlineAppSystem)> for S
where
    SChoice: TaskSystem<In = (), Out = Result<(), WokUnknownError>> + 'static,
    S: IntoBlockingSystem<Marker>,
    S::System: System<In = (), Out = Result<SystemEntry<SChoice>, WokUnknownError>>,
{
    unsafe fn app_run(
        self,
        state: &UnsafeMutState,
        center: &mut WorldCenter,
    ) -> impl Future<Output = Result<(), WokUnknownError>> + 'static {
        let system = center.register_system(self.into_system());

        let choice_result = {
            let mut world = unsafe { state.borrow_world_mut(&mut center.system_locks) };
            world
                .local_blocking()
                .run(system.entry_ref(), ())
                .expect("to run main app system")
        };

        let choice = match choice_result {
            Ok(choice) => choice,
            Err(err) => return futures::future::Either::Left(futures::future::ready(Err(err))),
        };

        let fut = {
            let mut world = unsafe { state.borrow_world_mut(&mut center.system_locks) };
            world
                .local_tasks()
                .run_dyn(choice.entry_ref(), ())
                .expect("to run main app system")
        };

        futures::future::Either::Right(fut.map(|(_, out)| out))
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
