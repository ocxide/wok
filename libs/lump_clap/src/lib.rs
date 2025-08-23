use std::{any::Any, collections::HashMap};

use clap::{ArgMatches, CommandFactory, FromArgMatches};
use lump::prelude::*;
use lump_core::schedule::ScheduleConfigure;

pub struct CommandRoot(pub clap::Command);
impl Resource for CommandRoot {}

type HandlerIn = Box<dyn Any + Send + Sync + 'static>;
type HandlerOut = Result<(), LumpUnknownError>;

struct Handler {
    parser: fn(&ArgMatches) -> Result<HandlerIn, clap::Error>,
    system: DynSystem<In<HandlerIn>, HandlerOut>,
}

impl Handler {
    fn new<Arg, S, Marker>(system: S) -> Self
    where
        Arg: FromArgMatches + Send + Sync + 'static,
        S: IntoSystem<Marker>,
        S::System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
    {
        let parser = |matches: &ArgMatches| {
            Arg::from_arg_matches(matches).map(|arg| Box::new(arg) as HandlerIn)
        };

        let system = (|data: In<HandlerIn>| {
            let data = data.0.downcast::<Arg>().expect("To be the same type");
            *data
        })
        .pipe_then(system);

        Self {
            parser,
            system: Box::new(system.into_system()),
        }
    }

    pub fn run(
        &self,
        matches: &ArgMatches,
        world: &WorldState,
    ) -> Result<impl Future<Output = HandlerOut>, clap::Error> {
        let arg = (self.parser)(matches)?;
        Ok(self.system.run(world, arg))
    }
}

#[derive(Default)]
pub struct Router {
    routes: HashMap<Box<[&'static str]>, Handler>,
}

impl Resource for Router {}
impl Router {
    fn add<const N: usize>(&mut self, route: [&'static str; N], handler: Handler) {
        self.routes.insert(route.into(), handler);
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
        app.insert_resource(CommandRoot(self.command))
            .init_resource::<Router>();
    }
}

pub struct MainHandler(Handler);
impl Resource for MainHandler {}

pub struct Main;
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
        let handler = Handler::new(system);
        world.insert_resource(MainHandler(handler));
    }
}

pub struct ClapInvoke {}
