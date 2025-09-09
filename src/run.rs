use lump_core::{
    error::LumpUnknownError,
    prelude::{IntoSystem, Res, ResMut, Resource, System},
    schedule::{ScheduleConfigure, ScheduleLabel, Systems},
};

use crate::{plugin::Plugin, prelude::SystemReserver};

#[derive(Copy, Clone)]
pub struct Run;
impl ScheduleLabel for Run {}

#[derive(Default)]
pub struct RunSystems(Systems<(), Result<(), LumpUnknownError>>);
impl Resource for RunSystems {}

#[doc(hidden)]
pub struct FallibleRun;

impl<Marker, S> ScheduleConfigure<S, (FallibleRun, Marker)> for Run
where
    S: IntoSystem<Marker>,
    S::System: System<In = (), Out = Result<(), LumpUnknownError>>,
{
    fn add(self, world: &mut lump_core::world::World, system: S) {
        let system = system.into_system();
        let id = world.register_system_ref(&system);

        let mut systems = world.state.get::<ResMut<RunSystems>>();
        systems.0.add(id, Box::new(system), ());
    }
}

#[doc(hidden)]
pub struct InfallibleRun;
impl<Marker, S> ScheduleConfigure<S, (InfallibleRun, Marker)> for Run
where
    S: IntoSystem<Marker>,
    S::System: System<In = (), Out = ()>,
{
    fn add(self, world: &mut lump_core::world::World, system: S) {
        self.add(world, system.map(|| Ok(())));
    }
}

pub async fn runtime(
    reserver: SystemReserver<'_>,
    systems: Res<'_, RunSystems>,
) -> Result<(), LumpUnknownError> {
    let locks = systems.0.iter().map(|(id, _, _)| reserver.clone().lock(id));

    let permits = futures::future::join_all(locks).await;
    let runs = permits
        .into_iter()
        .zip(systems.0.iter())
        .map(|(permit, (_, system, _))| permit.run_task(system, ()));

    futures::future::try_join_all(runs).await?;
    Ok(())
}

pub struct DefaultPlugins;

impl Plugin for DefaultPlugins {
    fn setup(self, app: impl crate::prelude::ConfigureApp) {
        app.init_resource::<RunSystems>();
    }
}
