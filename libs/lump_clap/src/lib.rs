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
    fn setup(self, app: impl ConfigureMoreWorld) {
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

fn make_route_handler<Arg: FromArgMatches + Send + Sync + 'static, Marker>(
    system: impl IntoSystem<Marker, System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>>,
) -> impl TaskSystem<In = HandlerIn, Out = HandlerOut> + ProtoSystem {
    (|matches: InRef<'_, ArgMatches>| Arg::from_arg_matches(&matches))
        .try_then(system)
        .into_system()
}

pub struct Route(&'static str);
impl ScheduleLabel for Route {}

pub struct RouteCfg<'r> {
    prefix: &'r [&'static str],
    world: &'r mut WorldCenter,
    router: &'r mut Router,
}

impl<F> ScheduleConfigure<F, ()> for Route
where
    F: FnOnce(&mut RouteCfg<'_>) + 'static,
{
    fn add(self, world: &mut lump_core::world::World, func: F) {
        let mut router = world
            .state
            .resources
            .get_mut::<Router>()
            .expect("to have a router");

        let mut cfg = RouteCfg {
            world: &mut world.center,
            prefix: &[self.0],
            router: &mut router,
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
        Arg: FromArgMatches + Send + Sync + 'static,
    {
        let system = make_route_handler(system);
        let id = self.world.register_system(&system);

        let mut route = self.prefix.to_vec();
        route.push(name);

        self.router.add(route, id, Box::new(system));

        self
    }

    pub fn cfg(&mut self, f: impl FnOnce(&mut RouteCfg<'_>)) -> &mut Self {
        (f)(self);
        self
    }

    pub fn nested(&mut self, name: &'static str, f: impl FnOnce(&mut RouteCfg<'_>)) -> &mut Self {
        let prefix = [self.prefix, &[name]].concat();
        let mut cfg = RouteCfg {
            prefix: &prefix,
            world: self.world,
            router: self.router,
        };
        f(&mut cfg);

        self
    }
}
