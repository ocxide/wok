use wok_core::{
    error::WokUnknownError,
    prelude::{BorrowMutParam, IntoSystem, ProtoTaskSystem, Res, ResMut, Resource, System},
    schedule::{ScheduleConfigure, ScheduleLabel, Systems},
    world::ConfigureWorld,
};

use crate::{app::App, plugin::Plugin, remote_gateway::RemoteWorldRef};

#[derive(Copy, Clone)]
pub struct Run;
impl ScheduleLabel for Run {}

#[derive(Default, Resource)]
#[resource(usage = lib, mutable = true)]
pub struct RunSystems(Systems<(), Result<(), WokUnknownError>>);

#[doc(hidden)]
pub struct FallibleRun;

impl<Marker, S> ScheduleConfigure<S, (FallibleRun, Marker)> for Run
where
    S: IntoSystem<Marker>,
    S::System:
        System<In = (), Out = Result<(), WokUnknownError>> + ProtoTaskSystem<Param: BorrowMutParam>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.into_system();
        let entry = world.register_system(system);

        let mut systems = world.state.get::<ResMut<RunSystems>>();
        systems.0.add(entry.into_taskbox(), ());
    }
}

#[doc(hidden)]
pub struct InfallibleRun;
impl<Marker, S> ScheduleConfigure<S, (InfallibleRun, Marker)> for Run
where
    S: IntoSystem<Marker>,
    S::System: System<In = (), Out = ()> + ProtoTaskSystem<Param: BorrowMutParam>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        self.add(world, system.map(|| Ok(())));
    }
}

pub async fn runtime(
    systems: Res<'_, RunSystems>,
    world: RemoteWorldRef<'_>,
) -> Result<(), WokUnknownError> {
    let world = world.upgrade().expect("to have a world");
    let reserver = world.reserver();

    let locks = systems
        .0
        .iter()
        .map(|(entry, _)| reserver.reserve(entry.entry_ref()));

    let permits = futures::future::join_all(locks).await;
    let runs = permits.into_iter().map(|permit| permit.task().run_dyn(()));

    futures::future::try_join_all(runs).await?;
    Ok(())
}

pub struct DefaultPlugins;

impl Plugin for DefaultPlugins {
    fn setup(self, app: &mut App) {
        app.init_resource::<RunSystems>();
    }
}
