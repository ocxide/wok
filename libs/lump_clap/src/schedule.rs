use clap::{ArgMatches, Args, FromArgMatches};
use lump::prelude::{ConfigureWorld, ResMut};
use lump_core::{
    prelude::{
        In, InRef, IntoBlockingSystem, IntoSystem, LumpUnknownError, ProtoSystem, Resource, System,
        TaskSystem,
    },
    schedule::{ScheduleConfigure, ScheduleLabel},
    world::{SystemId, WorldCenter},
};
use one_route::OneRoute;

use crate::router::{ClapHandler, HandlerIn, HandlerOut, Router};

pub use sub_routes::SubRoutes;

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

        let id = world.register_system_ref(&system);
        world.insert_resource(MainHandler(id, Box::new(system)));
    }
}

fn make_route_handler<Arg: FromArgMatches + Send + Sync + 'static, Marker>(
    system: impl IntoSystem<Marker, System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>>,
) -> impl TaskSystem<In = HandlerIn, Out = HandlerOut> + ProtoSystem {
    (|matches: InRef<'_, ArgMatches>| Arg::from_arg_matches(&matches))
        .try_then(system)
        .into_system()
}

pub struct Route(pub &'static str);

impl Route {
    pub fn mut_command(self, f: impl FnOnce(clap::Command) -> clap::Command) -> RouteCommand {
        RouteCommand(CommandInfo {
            name: self.0,
            command: f(clap::Command::new(self.0)),
        })
    }

    fn command(self) -> RouteCommand {
        RouteCommand(CommandInfo {
            name: self.0,
            command: clap::Command::new(self.0),
        })
    }
}

impl ScheduleLabel for Route {}

#[doc(hidden)]
pub struct SingleRoute;
impl<Arg, S, Marker> ScheduleConfigure<S, (Arg, Marker, SingleRoute)> for Route
where
    Arg: FromArgMatches + Args + Send + Sync + 'static,
    S: IntoSystem<Marker>,
    S::System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
{
    fn add(self, world: &mut lump_core::world::World, system: S) {
        self.command().add(world, system);
    }
}

#[doc(hidden)]
pub struct MultiRoute;
impl<R> ScheduleConfigure<R, MultiRoute> for Route
where
    R: ConfigureRoute,
{
    fn add(self, world: &mut lump_core::world::World, route: R) {
        self.command().add(world, route);
    }
}

pub struct RouteCommand(pub CommandInfo);

impl<Arg, S, Marker> ScheduleConfigure<S, (Arg, Marker, SingleRoute)> for RouteCommand
where
    Arg: FromArgMatches + Args + Send + Sync + 'static,
    S: IntoSystem<Marker>,
    S::System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
{
    fn add(self, world: &mut lump_core::world::World, system: S) {
        let route = OneRoute::new(system);
        self.add(world, route);
    }
}

impl<R> ScheduleConfigure<R, MultiRoute> for RouteCommand
where
    R: ConfigureRoute,
{
    fn add(self, world: &mut lump_core::world::World, route: R) {
        let (mut command_root, mut router) = world
            .state
            .get::<(ResMut<'_, CommandRoot>, ResMut<'_, Router>)>();

        let mut command_root = CommmandMut(command_root.0.as_mut().expect("command root"));

        let subroute = SubRoute::new(self.0, route);
        subroute.sub(&[], &mut world.center, &mut command_root, &mut router);
    }
}

pub struct CommandInfo {
    pub name: &'static str,
    pub command: clap::Command,
}

pub struct CommmandMut<'m>(&'m mut clap::Command);

impl<'m> CommmandMut<'m> {
    pub fn mutate(&mut self, f: impl FnOnce(clap::Command) -> clap::Command) {
        take_mut::take(self.0, f);
    }
}

pub trait ConfigureRoute {
    fn one(
        self,
        route: &[&'static str],
        center: &mut WorldCenter,
        command: &mut CommmandMut<'_>,
        router: &mut Router,
    );
}

pub(crate) mod cardinality {
    pub struct OneOrMore;
    pub struct ZeroOrMore;
}

pub trait ConfigureRoutesSet {
    type Cardinality;

    fn set(
        self,
        route: &[&'static str],
        center: &mut WorldCenter,
        command: &mut CommmandMut<'_>,
        router: &mut Router,
    );
}

impl ConfigureRoutesSet for () {
    type Cardinality = cardinality::ZeroOrMore;

    fn set(
        self,
        _route: &[&'static str],
        _center: &mut WorldCenter,
        _command: &mut CommmandMut<'_>,
        _router: &mut Router,
    ) {
    }
}

impl<M: ConfigureRoutesSet, C> ConfigureRoutesSet for (M, SubRoute<C>)
where
    C: ConfigureRoute,
{
    type Cardinality = cardinality::OneOrMore;

    fn set(
        self,
        route: &[&'static str],
        center: &mut WorldCenter,
        command: &mut CommmandMut<'_>,
        router: &mut Router,
    ) {
        self.0.set(route, center, command, router);
        self.1.sub(route, center, command, router);
    }
}

pub struct SubRoute<C: ConfigureRoute> {
    command: CommandInfo,
    config: C,
}

impl<C: ConfigureRoute> SubRoute<C> {
    fn sub(
        mut self,
        route: &[&'static str],
        center: &mut WorldCenter,
        command: &mut CommmandMut<'_>,
        router: &mut Router,
    ) {
        let sub_route = [route, &[self.command.name]].concat();
        let mut sub_command = CommmandMut(&mut self.command.command);

        self.config
            .one(&sub_route, center, &mut sub_command, router);

        command.mutate(|c| c.subcommand(self.command.command));
    }
}

impl<C: ConfigureRoute> SubRoute<C> {
    pub fn new(command: CommandInfo, config: C) -> Self {
        SubRoute { command, config }
    }
}

mod sub_routes {
    use clap::{Args, FromArgMatches};
    use lump::prelude::{In, IntoSystem, LumpUnknownError, System};
    use lump_core::world::WorldCenter;

    use crate::router::Router;

    use super::{
        CommandInfo, CommmandMut, ConfigureRoute, ConfigureRoutesSet, Route, SubRoute, cardinality,
        one_route::OneRoute,
    };

    pub struct SubRoutes<Routes = ()>(Routes);

    impl SubRoutes<()> {
        pub const fn empty() -> Self {
            SubRoutes(())
        }
    }

    impl Default for SubRoutes<()> {
        fn default() -> Self {
            SubRoutes(())
        }
    }

    impl<Routes: ConfigureRoutesSet<Cardinality = cardinality::OneOrMore>> ConfigureRoute
        for SubRoutes<Routes>
    {
        fn one(
            self,
            prefix: &[&'static str],
            center: &mut WorldCenter,
            command: &mut CommmandMut<'_>,
            router: &mut Router,
        ) {
            self.0.set(prefix, center, command, router);
        }
    }

    impl<Routes: ConfigureRoutesSet> SubRoutes<Routes> {
        pub fn add<Marker, Arg, S>(
            self,
            route: Route,
            system: S,
        ) -> SubRoutes<impl ConfigureRoutesSet<Cardinality = cardinality::OneOrMore>>
        where
            Arg: FromArgMatches + Args + Send + Sync + 'static,
            S: IntoSystem<Marker, System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>>,
        {
            let cfg = SubRoute::new(
                CommandInfo {
                    name: route.0,
                    command: clap::Command::new(route.0),
                },
                OneRoute::new(system),
            );
            SubRoutes((self.0, cfg))
        }

        pub fn sub(
            self,
            route: Route,
            sub: SubRoutes<impl ConfigureRoutesSet<Cardinality = cardinality::OneOrMore>>,
        ) -> SubRoutes<impl ConfigureRoutesSet<Cardinality = cardinality::OneOrMore>> {
            let cfg = SubRoute::new(
                CommandInfo {
                    name: route.0,
                    command: clap::Command::new(route.0),
                },
                sub,
            );
            SubRoutes((self.0, cfg))
        }
    }
}

mod one_route {
    use clap::{Args, FromArgMatches};
    use lump::prelude::{In, IntoSystem, LumpUnknownError, System};

    use super::{ConfigureRoute, make_route_handler};

    pub struct OneRoute<S, Marker> {
        system: S,
        _marker: std::marker::PhantomData<Marker>,
    }

    impl<S, Arg, Marker> OneRoute<S, (Arg, Marker)>
    where
        Arg: FromArgMatches + Args + Send + Sync + 'static,
        S: IntoSystem<Marker>,
        S::System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
    {
        pub fn new(system: S) -> Self {
            Self {
                system,
                _marker: std::marker::PhantomData,
            }
        }
    }

    impl<Arg, Marker, S> ConfigureRoute for OneRoute<S, (Arg, Marker)>
    where
        Arg: FromArgMatches + Args + Send + Sync + 'static,
        S: IntoSystem<Marker>,
        S::System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>,
    {
        fn one(
            self,
            route: &[&'static str],
            center: &mut lump_core::world::WorldCenter,
            command: &mut super::CommmandMut<'_>,
            router: &mut crate::router::Router,
        ) {
            let system = make_route_handler(self.system);
            let id = center.register_system(&system);

            command.mutate(|c| Arg::augment_args(c));
            router.add(route, id, Box::new(system));
        }
    }
}
