use std::collections::HashMap;

use clap::{ArgMatches, CommandFactory, FromArgMatches};
use lump::prelude::*;
use lump_core::{
    schedule::{ScheduleConfigure, ScheduleLabel},
    world::{SystemId, WorldCenter},
};

type ClapHandler = DynSystem<HandlerIn, HandlerOut>;
type HandlerIn = InRef<'static, ArgMatches>;
type HandlerOut = Result<Result<(), LumpUnknownError>, clap::error::Error>;

pub struct CommandRoot(pub Option<clap::Command>);
impl Resource for CommandRoot {}

#[derive(Default)]
pub struct Router {
    routes: HashMap<Box<[&'static str]>, (SystemId, ClapHandler)>,
}

impl Resource for Router {}
impl Router {
    fn add(
        &mut self,
        route: impl Into<Box<[&'static str]>>,
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
    fn setup(self, app: impl ConfigureApp) {
        app.insert_resource(CommandRoot(Some(self.command)))
            .init_resource::<Router>();
    }
}

pub struct MainHandler(SystemId, ClapHandler);
impl Resource for MainHandler {}

pub struct Main;

impl ScheduleLabel for Main {}

impl<Arg: FromArgMatches + Send + Sync + 'static, S, Marker> ScheduleConfigure<S, Marker> for Main
where
    S: IntoSystem<Marker>,
    S: 'static,
    S::System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
{
    fn add(self, world: &mut lump_core::world::World, system: S) {
        let system = make_route_handler(system);

        let id = world.register_system(&system);
        world.insert_resource(MainHandler(id, Box::new(system)));
    }
}

pub async fn clap_runtime(
    reserver: SystemReserver<'_>,
    main: Option<Res<'_, MainHandler>>,
    mut command: ResMut<'_, CommandRoot>,
    router: Res<'_, Router>,
) -> Result<(), LumpUnknownError> {
    let mut command = command.0.take().expect("to have a command");

    if main.is_none() {
        command = command.subcommand_required(true);
    }

    command.build();

    let args = match command.try_get_matches() {
        Ok(args) => args,
        Err(err) => {
            println!("{}", err);
            return Ok(());
        }
    };

    if let Some(MainHandler(id, main)) = main.as_deref() {
        let res = match reserver.lock(*id).await.run_task(main, &args).await {
            Ok(res) => res,
            Err(err) => {
                println!("{}", err);
                return Ok(());
            }
        };

        return res;
    }

    let (name, mut sub_args) = match args.subcommand() {
        Some((name, args)) => (name, args),
        None => {
            return Ok(());
        }
    };

    let mut rotue = vec![name];

    while let Some((name, matches)) = sub_args.subcommand() {
        rotue.push(name);
        sub_args = matches;
    }

    if let Some((id, system)) = router.routes.get(rotue.as_slice()) {
        let res = match reserver.lock(*id).await.run_task(system, sub_args).await {
            Ok(res) => res,
            Err(err) => {
                println!("{}", err);
                return Ok(());
            }
        };

        return res;
    }

    Ok(())
}

fn make_route_handler<Arg: FromArgMatches + Send + Sync + 'static, Marker>(
    system: impl IntoSystem<Marker, System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>>,
) -> impl TaskSystem<In = HandlerIn, Out = HandlerOut> + ProtoSystem {
    (|matches: InRef<'_, ArgMatches>| Arg::from_arg_matches(&matches))
        .try_then(system)
        .into_system()
}

pub struct Route(pub &'static str);
impl ScheduleLabel for Route {}

pub struct RouteCfg<'r> {
    prefix: &'r [&'static str],
    world: &'r mut WorldCenter,
    router: &'r mut Router,
    command: &'r mut clap::Command,
}

impl<F> ScheduleConfigure<F, ()> for Route
where
    F: FnOnce(&mut RouteCfg<'_>) + 'static,
{
    fn add(self, world: &mut lump_core::world::World, func: F) {
        let (mut router, mut command) = world.state.get::<(ResMut<Router>, ResMut<CommandRoot>)>();
        let command = command.0.as_mut().expect("to have a command");

        let mut cfg = RouteCfg {
            world: &mut world.center,
            prefix: &[self.0],
            router: &mut router,
            command,
        };
        func(&mut cfg);
    }
}

impl RouteCfg<'_> {
    pub fn add<Marker, Arg, S>(
        &mut self,
        name: &'static str,
        system: impl IntoSystem<
            Marker,
            System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
        >,
    ) -> &mut Self
    where
        Arg: FromArgMatches + CommandFactory + Send + Sync + 'static,
    {
        let system = make_route_handler(system);
        let id = self.world.register_system(&system);

        let mut route = self.prefix.to_vec();
        route.push(name);

        self.router.add(route, id, Box::new(system));

        let mut subcommand = Arg::command();
        take_mut::take(&mut subcommand, |command| command.name(name));

        take_mut::take(self.command, move |command| command.subcommand(subcommand));

        self
    }

    pub fn cfg(&mut self, f: impl FnOnce(&mut RouteCfg<'_>)) -> &mut Self {
        (f)(self);
        self
    }

    pub fn nested(&mut self, name: &'static str, f: impl FnOnce(&mut RouteCfg<'_>)) -> &mut Self {
        let prefix = [self.prefix, &[name]].concat();

        let mut subcommand = clap::Command::new(name);
        let mut cfg = RouteCfg {
            prefix: &prefix,
            world: self.world,
            router: self.router,
            command: &mut subcommand,
        };
        f(&mut cfg);

        take_mut::take(self.command, |command| command.subcommand(subcommand));
        self
    }

    pub fn finish(&mut self) {}

    pub fn single<Marker, Arg>(
        &mut self,
        system: impl IntoSystem<
            Marker,
            System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
        >,
    ) where
        Arg: FromArgMatches + CommandFactory + Send + Sync + 'static,
    {
        let system = make_route_handler(system);
        let id = self.world.register_system(&system);

        self.router.add(self.prefix, id, Box::new(system));

        let name = self.prefix.last().expect("to have a name");

        let mut subcommand = Arg::command();
        take_mut::take(&mut subcommand, |command| command.name(name));
        take_mut::take(self.command, move |command| command.subcommand(subcommand));
    }
}
