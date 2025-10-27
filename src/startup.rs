use futures::{StreamExt, stream::FuturesUnordered};
use wok_core::{
    error::WokUnknownError,
    prelude::{
        DynBlockingSystem, IntoBlockingSystem, IntoSystem, ResMut, Resource, System, TaskSystem,
    },
    schedule::{
        ScheduleConfigure, ScheduleLabel, dependency_graph::SystemsMutationDependencyGraph,
    },
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

type DynTaskSystem<In, Out> = Box<dyn TaskSystem<In = In, Out = Out> + Send + Sync>;

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
    S::System: System<In = (), Out = Result<(), WokUnknownError>>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.into_system();
        let systemid = world.register_system_ref(&system);

        let mut systems = world.get::<ResMut<StartupSystems>>();

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
    S::System: System<In = (), Out = ()>,
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

        let mut systems = world.get::<ResMut<StartupSystems>>();

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

        let mut systems = world.get::<ResMut<StartupSystems>>();

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

        let graph = wok_core::schedule::dependency_graph::build_sequencial_graph(
            &systems.pendings,
            &center.system_locks.systems_rw,
        )
        .expect("Dependency graph build failed");

        StartupInvoke {
            center,
            rt,
            state,
            systems,
            futures: FuturesUnordered::new(),
            inline_finshed: vec![],
            graph,
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
    inline_finshed: Vec<SystemId>,
    graph: SystemsMutationDependencyGraph,
}

impl<'w, C: AsyncExecutor> StartupInvoke<'w, C> {
    fn collect_pending_systems(&mut self) -> Result<(), WokUnknownError> {
        let Self {
            center,
            rt,
            state,
            systems,
            futures,
            graph,
            inline_finshed,
        } = self;

        for id in systems.pendings.iter().copied() {
            let has_pendings = graph
                .get_dependencies(id)
                .map(|dependencies| {
                    // is any of out dependencies in pendings?
                    dependencies.iter().any(|id| systems.pendings.contains(id))
                })
                .unwrap_or(false);

            if has_pendings {
                continue;
            }

            let system = systems.systems.get(&id).expect("System not registered");
            let mut world = WorldMut::new(state, &mut center.system_locks);

            match system {
                StartupSystem::Async(system) => {
                    let permit = match world.reserve(SystemEntryRef::new(id, system)) {
                        Ok(permit) => permit,
                        _ => continue,
                    };

                    let fut = permit.local_tasks().run_dyn(());

                    let fut = rt.spawn(fut);
                    futures.push(fut);
                }

                StartupSystem::Blocking(system) => {
                    let permit = match world.reserve(SystemEntryRef::new(id, system)) {
                        Ok(permit) => permit,
                        _ => continue,
                    };

                    let caller = permit.local_blocking().create_caller();
                    let fut = rt.spawn_blocking(move || {
                        let out = caller.run(());
                        (id, out)
                    });
                    futures.push(fut);
                }

                StartupSystem::Inline(system) => {
                    let permit = match world.reserve(SystemEntryRef::new(id, system)) {
                        Ok(permit) => permit,
                        _ => continue,
                    };

                    permit.local_blocking().run_dyn(())?;
                    inline_finshed.push(id);
                }
            }
        }

        Ok(())
    }

    pub async fn invoke(mut self) -> Result<(), WokUnknownError> {
        loop {
            self.collect_pending_systems()?;

            self.center.tick_commands(self.state);
            for systemid in self.inline_finshed.drain(..) {
                Self::on_finish(systemid, &mut self.systems.pendings);
            }

            if let Some(Ok((systemid, result))) = self.futures.next().await {
                self.center.system_locks.release(systemid);

                self.center.tick_commands(self.state);
                Self::on_finish(systemid, &mut self.systems.pendings);
                result?;
            } else if self.systems.pendings.is_empty() {
                break;
            }
        }

        Ok(())
    }

    fn on_finish(systemid: SystemId, pendings: &mut Vec<SystemId>) {
        if let Some(index) = pendings.iter().position(|id| *id == systemid) {
            pendings.remove(index);
        }
    }
}
