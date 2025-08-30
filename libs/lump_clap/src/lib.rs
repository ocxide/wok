use std::collections::HashMap;

use clap::{ArgMatches, CommandFactory, FromArgMatches};
use lump::prelude::*;
use lump_core::{schedule::{ScheduleConfigure, ScheduleLabel}, world::SystemId};

type ClapHandler =
    DynSystem<InRef<'static, ArgMatches>, Result<Result<(), LumpUnknownError>, clap::error::Error>>;

pub struct CommandRoot(pub Option<clap::Command>);
impl Resource for CommandRoot {}

#[derive(Default)]
pub struct Router {
    routes: HashMap<Box<[&'static str]>, (SystemId, ClapHandler)>,
}

impl Resource for Router {}
impl Router {
    fn add<const N: usize>(
        &mut self,
        route: [&'static str; N],
        system_id: SystemId,
        handler: ClapHandler,
    ) {
        self.routes.insert(route.into(), (system_id, handler));
    }
}

pub struct ClapPlugin {
    command: clap::Command,
}

impl ClapPlugin {
    pub fn parser<F: CommandFactory>() -> Self {
        Self {
            command: F::command(),
        }
    }
}

impl Plugin for ClapPlugin {
    fn setup(self, app: impl ConfigureMoreWorld) {
        app.insert_resource(CommandRoot(Some(self.command)))
            .init_resource::<Router>();
    }
}

pub struct MainHandler(SystemId, ClapHandler);
impl Resource for MainHandler {}

#[derive(Clone, Copy)]
pub struct Main;

impl ScheduleLabel for Main {}

impl<Arg: FromArgMatches + Send + Sync + 'static>
    ScheduleConfigure<In<Arg>, Result<(), LumpUnknownError>> for Main
{
    fn add<Marker>(
        world: &mut lump_core::world::World,
        system: impl IntoSystem<
            Marker,
            System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
        >,
    ) {
        let system = (|matches: InRef<'_, ArgMatches>| Arg::from_arg_matches(&matches))
            .try_then(system)
            .into_system();

        let id = world.register_system(&system);
        world.insert_resource(MainHandler(id, Box::new(system)));
    }
}

pub async fn clap_runtime(
    reserver: SystemReserver<'_>,
    main: Option<Res<'_, MainHandler>>,
    mut command: ResMut<'_, CommandRoot>,
) -> Result<(), LumpUnknownError> {
    let mut command = command.0.take().expect("to have a command");
    command.build();

    let args = command.try_get_matches()?;

    if let Some(MainHandler(id, main)) = main.as_deref() {
        let res = reserver.lock(*id).await.run_task(main, &args).await?;
        return res;
    }

    // let mut sub_args: MaybeUninit<_>;
    // let mut rotue = vec![];
    // while let Some((name, matches)) = args.subcommand() {
    //     rotue.push(name);
    //     sub_args = MaybeUninit::new(matches);
    // }
    //
    // if sub_args
    //
    // if let Some((id, system)) = router.routes.get(rotue.as_slice()) {
    //     let res = reserver
    //         .lock(*id)
    //         .await
    //         .run_task(system, &sub_args)
    //         .await?;
    //
    //     return res;
    // }

    Ok(())
}
