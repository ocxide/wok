use futures::{StreamExt, stream::FuturesUnordered};
use lump_core::{
    error::LumpUnknownError,
    prelude::{IntoSystem, System},
    resources::LocalResource,
    schedule::{ScheduleConfigure, ScheduleLabel, SystemsMap},
    world::{SystemId, WorldCenter, WorldState, WorldSystemLockError},
};

use lump_core::async_executor::AsyncExecutor;

#[derive(Default)]
struct StartupSystems {
    systems: SystemsMap<(), Result<(), LumpUnknownError>>,
    pendings: Vec<SystemId>,
}

impl LocalResource for StartupSystems {}

#[derive(Copy, Clone)]
pub struct Startup;

impl ScheduleLabel for Startup {}

#[doc(hidden)]
pub struct FallibleStartup;
impl<Marker, S> ScheduleConfigure<S, (FallibleStartup, Marker)> for Startup
where
    S: IntoSystem<Marker> + 'static,
    S::System: System<In = (), Out = Result<(), LumpUnknownError>>,
{
    fn add(self, world: &mut lump_core::world::World, system: S) {
        let system = system.into_system();
        let systemid = world.register_system(&system);

        let systems = world
            .center
            .resources
            .get_mut::<StartupSystems>()
            .expect("Startup schedule was not initialized");

        systems.systems.add_system(systemid, Box::new(system));
        systems.pendings.push(systemid);
    }
}

#[doc(hidden)]
pub struct InfallibleStartup;
impl<Marker, S> ScheduleConfigure<S, (InfallibleStartup, Marker)> for Startup
where
    S: IntoSystem<Marker> + 'static,
    S::System: System<In = (), Out = ()>,
{
    fn add(self, world: &mut lump_core::world::World, system: S) {
        let system = system.map(|| Ok(()));
        self.add(world, system);
    }
}

impl Startup {
    pub fn init(world: &mut WorldCenter) {
        world.resources.init::<StartupSystems>();
    }

    pub fn create_invoker<'w, C: AsyncExecutor>(
        center: &'w mut WorldCenter,
        state: &'w mut WorldState,
        rt: &'w C,
    ) -> StartupInvoke<'w, C> {
        let systems = center
            .resources
            .try_take::<StartupSystems>()
            .expect("Startup schedule was not initialized");

        StartupInvoke {
            center,
            rt,
            state,
            systems,
            futures: FuturesUnordered::new(),
        }
    }
}

type FutJoinHandle<C> = <C as AsyncExecutor>::JoinHandle<(SystemId, Result<(), LumpUnknownError>)>;
pub struct StartupInvoke<'w, C: AsyncExecutor> {
    center: &'w mut WorldCenter,
    rt: &'w C,
    state: &'w mut WorldState,
    systems: StartupSystems,
    futures: FuturesUnordered<FutJoinHandle<C>>,
}

impl<'w, C: AsyncExecutor> StartupInvoke<'w, C> {
    fn collect_pending_systems(&mut self) {
        let Self {
            center,
            rt,
            state,
            systems,
            futures,
        } = self;

        for _ in systems.pendings.extract_if(.., |id| {
            let id = *id;
            let system = match systems.systems.get(id) {
                Some(system) => system,
                None => return false,
            };

            match center.system_locks.try_lock(id) {
                Ok(_) => {}
                Err(WorldSystemLockError::NotRegistered) => {
                    panic!("System not registered")
                }
                Err(WorldSystemLockError::InvalidAccess) => return false,
            };

            // Already checked with locks; TODO: Come up with better api
            let fut = unsafe { system.run(state, ()) };
            let fut = rt.spawn(async move {
                let result = fut.await;
                (id, result)
            });

            futures.push(fut);

            true
        }) {}
    }

    pub async fn invoke(mut self) -> Result<(), LumpUnknownError> {
        self.collect_pending_systems();

        while let Some(Ok((systemid, result))) = self.futures.next().await {
            self.center.system_locks.release(systemid);
            self.center.tick_commands(self.state);

            result?;

            self.collect_pending_systems();
        }

        Ok(())
    }
}
