use clap::{ArgMatches, CommandFactory, FromArgMatches};
use lump::prelude::{ConfigureWorld, ResMut};
use lump_core::{
    prelude::{
        In, InRef, IntoBlockingSystem, IntoSystem, LumpUnknownError, ProtoSystem, Resource,
        System, TaskSystem,
    },
    schedule::{ScheduleConfigure, ScheduleLabel},
    world::{SystemId, WorldCenter},
};

use crate::router::{ClapHandler, HandlerIn, HandlerOut, Router};

pub struct CommandRoot(pub Option<clap::Command>);
impl Resource for CommandRoot {}

pub struct MainHandler(pub SystemId, pub ClapHandler);
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

fn make_route_handler<Arg: FromArgMatches + Send + Sync + 'static, Marker>(
    system: impl IntoSystem<
        Marker,
        System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
    >,
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
        let (mut router, mut command) =
            world.state.get::<(ResMut<Router>, ResMut<CommandRoot>)>();
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

    pub fn nested(
        &mut self,
        name: &'static str,
        f: impl FnOnce(&mut RouteCfg<'_>),
    ) -> &mut Self {
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

