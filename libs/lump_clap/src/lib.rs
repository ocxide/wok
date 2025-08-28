use std::{any::Any, collections::HashMap};

use clap::{ArgMatches, CommandFactory, FromArgMatches};
use lump::{foreign::ParamsClient, prelude::*};
use lump_core::schedule::ScheduleConfigure;

pub struct CommandRoot(pub Option<clap::Command>);
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
        client: &ParamsClient,
    ) -> Result<impl Future<Output = HandlerOut>, clap::Error> {
        let arg = (self.parser)(matches)?;
        Ok(client.clone().run(self.system, arg))
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
        app.insert_resource(CommandRoot(Some(self.command)))
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

pub async fn clap_runtime(
    main: Option<Res<'_, MainHandler>>,
    router: Res<'_, Router>,
    mut command: ResMut<'_, CommandRoot>,
    client: Res<'_, ParamsClient>
) -> Result<(), LumpUnknownError> {
    command.0.build();
    let args = command.0.take().expect("to have a command").try_get_matches()?;

    if let Some(main) = main {
        let res = main.0.run(&args, &client.state).await?;
        return res;
    }

    Ok(())
}
