use futures::{StreamExt, stream::FuturesUnordered};
use wok_core::{
    error::WokUnknownError,
    prelude::{
        BorrowMutParam, BorrowTaskSystem, DynBlockingSystem, DynTaskSystem, IntoBlockingSystem,
        IntoSystem, ProtoTaskSystem, ResMut, Resource, System,
    },
    schedule::{ScheduleConfigure, ScheduleLabel},
    world::{
        ConfigureWorld, SystemId, World, WorldCenter, WorldState,
        gateway::{SystemEntryRef, WorldMut},
    },
};

use wok_core::async_executor::AsyncExecutor;

#[derive(Default, Resource)]
#[resource(usage = lib, mutable = true)]
struct StartupSystems {
    systems: std::collections::HashMap<SystemId, StartupSystem>,
    pendings: Vec<SystemId>,
}

enum StartupSystem {
    Async(DynTaskSystem<(), Result<(), WokUnknownError>>),
    Blocking(DynBlockingSystem<(), Result<(), WokUnknownError>>),
    Inline(DynBlockingSystem<(), Result<(), WokUnknownError>>),
}

#[derive(Copy, Clone)]
pub struct Startup;
impl ScheduleLabel for Startup {}

pub struct InlineStartup;
impl ScheduleLabel for InlineStartup {}

#[doc(hidden)]
pub struct FallibleStartup;
impl<Marker, S> ScheduleConfigure<S, (FallibleStartup, Marker)> for Startup
where
    S: IntoSystem<Marker> + 'static,
    S::System: System<In = (), Out = Result<(), WokUnknownError>> + BorrowTaskSystem,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.into_system();
        let systemid = world.register_system_ref(&system);

        let mut systems = world.state.get::<ResMut<StartupSystems>>();

        systems
            .systems
            .insert(systemid, StartupSystem::Async(Box::new(system)));
        systems.pendings.push(systemid);
    }
}

#[doc(hidden)]
pub struct InfallibleStartup;
impl<Marker, S> ScheduleConfigure<S, (InfallibleStartup, Marker)> for Startup
where
    S: IntoSystem<Marker> + 'static,
    S::System: System<In = (), Out = ()> + ProtoTaskSystem<Param: BorrowMutParam>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.map(|| Ok(()));
        self.add(world, system);
    }
}

#[doc(hidden)]
pub struct BlockingStartup;

impl<Marker, S> ScheduleConfigure<S, (FallibleStartup, BlockingStartup, Marker)> for Startup
where
    S: IntoBlockingSystem<Marker>,
    S::System: System<In = (), Out = Result<(), WokUnknownError>>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.into_system();
        let systemid = world.register_system_ref(&system);

        let mut systems = world.state.get::<ResMut<StartupSystems>>();

        systems
            .systems
            .insert(systemid, StartupSystem::Blocking(Box::new(system)));
        systems.pendings.push(systemid);
    }
}

impl<Marker, S> ScheduleConfigure<S, (InfallibleStartup, BlockingStartup, Marker)> for Startup
where
    S: IntoBlockingSystem<Marker>,
    S::System: System<In = (), Out = ()>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.pipe(|| Ok(()));
        self.add(world, system);
    }
}

impl<Marker, S> ScheduleConfigure<S, (InlineStartup, Marker)> for InlineStartup
where
    S: IntoBlockingSystem<Marker>,
    S::System: System<In = (), Out = Result<(), WokUnknownError>>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.into_system();
        let systemid = world.register_system_ref(&system);

        let mut systems = world.state.get::<ResMut<StartupSystems>>();

        systems
            .systems
            .insert(systemid, StartupSystem::Inline(Box::new(system)));
        systems.pendings.push(systemid);
    }
}

impl<Marker, S> ScheduleConfigure<S, (InfallibleStartup, InlineStartup, Marker)> for InlineStartup
where
    S: IntoBlockingSystem<Marker>,
    S::System: System<In = (), Out = ()>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.pipe(|| Ok(()));
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
            last_inline: None,
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
    last_inline: Option<(SystemId, Result<(), WokUnknownError>)>,
}

impl<'w, C: AsyncExecutor> StartupInvoke<'w, C> {
    fn collect_pending_systems(&mut self) {
        let Self {
            center,
            rt,
            state,
            systems,
            futures,
            last_inline,
        } = self;

        for _ in systems.pendings.extract_if(.., |id| {
            let system = match systems.systems.get(id) {
                Some(system) => system,
                None => return false,
            };

            let mut world = WorldMut::new(state.as_unsafe_world_state(), &mut center.system_locks);

            match system {
                StartupSystem::Async(system) => {
                    let fut = match world
                        .local_tasks()
                        .run_dyn(SystemEntryRef::new(*id, system), ())
                    {
                        Ok(fut) => fut,
                        _ => return false,
                    };

                    let fut = rt.spawn(fut);
                    futures.push(fut);
                }

                StartupSystem::Blocking(system) => {
                    let caller = match world
                        .local_inline()
                        .create_caller(SystemEntryRef::new(*id, system))
                    {
                        Ok(caller) => caller,
                        _ => return false,
                    };

                    let id = *id;
                    let fut = rt.spawn_blocking(move || {
                        let out = caller.run(());
                        (id, out)
                    });
                    futures.push(fut);
                }

                StartupSystem::Inline(system) => {
                    let id = *id;
                    let out = match world
                        .local_inline()
                        .run_dyn(SystemEntryRef::new(id, system), ())
                    {
                        Ok(out) => out,
                        _ => return false,
                    };

                    *last_inline = Some((id, out));
                }
            }

            true
        }) {}
    }

    pub async fn invoke(mut self) -> Result<(), WokUnknownError> {
        self.collect_pending_systems();

        loop {
            if self.systems.pendings.is_empty() {
                break;
            };

            if let Some((systemid, result)) = self.last_inline.take() {
                Self::on_finish(systemid, self.center, self.state);
                result?;
            };

            if let Some(Ok((systemid, result))) = self.futures.next().await {
                Self::on_finish(systemid, self.center, self.state);
                result?;
            } else {
                break;
            }

            self.collect_pending_systems();
        }

        Ok(())
    }

    fn on_finish(systemid: SystemId, center: &mut WorldCenter, state: &mut WorldState) {
        center.system_locks.release(systemid);
        center.tick_commands(state);
    }
}
