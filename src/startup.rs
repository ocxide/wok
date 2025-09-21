use futures::{StreamExt, stream::FuturesUnordered};
use wok_core::{
    error::WokUnknownError,
    prelude::{IntoSystem, ResMut, Resource, System},
    schedule::{ScheduleConfigure, ScheduleLabel, SystemsMap},
    world::gateway::{SystemEntryRef, WorldMut},
    world::{ConfigureWorld, SystemId, World, WorldCenter, WorldState},
};

use wok_core::async_executor::AsyncExecutor;

#[derive(Default, Resource)]
#[resource(usage = lib, mutable = true)]
struct StartupSystems {
    systems: SystemsMap<(), Result<(), WokUnknownError>>,
    pendings: Vec<SystemId>,
}

#[derive(Copy, Clone)]
pub struct Startup;

impl ScheduleLabel for Startup {}

#[doc(hidden)]
pub struct FallibleStartup;
impl<Marker, S> ScheduleConfigure<S, (FallibleStartup, Marker)> for Startup
where
    S: IntoSystem<Marker> + 'static,
    S::System: System<In = (), Out = Result<(), WokUnknownError>>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.into_system();
        let systemid = world.register_system_ref(&system);

        let mut systems = world.state.get::<ResMut<StartupSystems>>();

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
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.map(|| Ok(()));
        self.add(world, system);
    }
}

impl Startup {
    pub fn init(world: &mut World) {
        world.init_resource::<StartupSystems>();
    }

    pub fn create_invoker<'w, C: AsyncExecutor>(
        center: &'w mut WorldCenter,
        state: &'w mut WorldState,
        rt: &'w C,
    ) -> StartupInvoke<'w, C> {
        let systems = state
            .take_resource::<StartupSystems>()
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

type FutJoinHandle<C> = <C as AsyncExecutor>::JoinHandle<(SystemId, Result<(), WokUnknownError>)>;
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

            let mut world = WorldMut::new(state.as_unsafe_world_state(), &mut center.system_locks);
            let fut = match world
                .local_tasks()
                .run_dyn(SystemEntryRef::new(id, system), ())
            {
                Ok(fut) => fut,
                _ => return false,
            };
            let fut = rt.spawn(fut);

            futures.push(fut);

            true
        }) {}
    }

    pub async fn invoke(mut self) -> Result<(), WokUnknownError> {
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
