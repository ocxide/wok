use clap::{ArgMatches, CommandFactory, FromArgMatches};
use lump::prelude::{ConfigureWorld, ResMut};
use lump_core::{
    prelude::{
        In, InRef, IntoBlockingSystem, IntoSystem, LumpUnknownError, ProtoSystem, Resource, System,
        TaskSystem,
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
    system: impl IntoSystem<Marker, System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>>,
) -> impl TaskSystem<In = HandlerIn, Out = HandlerOut> + ProtoSystem {
    (|matches: InRef<'_, ArgMatches>| Arg::from_arg_matches(&matches))
        .try_then(system)
        .into_system()
}

pub struct Route(pub &'static str);
impl ScheduleLabel for Route {}

pub struct RoutesCfg<'r> {
    prefix: &'r [&'static str],
    world: &'r mut WorldCenter,
    router: &'r mut Router,
    command: &'r mut clap::Command,
}

impl<F> ScheduleConfigure<F, ()> for Route
where
    F: FnOnce(&mut RoutesCfg<'_>),
{
    fn add(self, world: &mut lump_core::world::World, func: F) {
        let (mut router, mut command) = world.state.get::<(ResMut<Router>, ResMut<CommandRoot>)>();
        let command = command.0.as_mut().expect("to have a command");

        let mut cfg = RoutesCfg {
            world: &mut world.center,
            prefix: &[self.0],
            router: &mut router,
            command,
        };
        func(&mut cfg);
    }
}

impl RoutesCfg<'_> {
    pub fn add<Marker, Arg>(
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

    pub fn cfg(&mut self, f: impl FnOnce(&mut RoutesCfg<'_>)) -> &mut Self {
        (f)(self);
        self
    }

    pub fn nested(&mut self, name: &'static str, f: impl FnOnce(&mut RoutesCfg<'_>)) -> &mut Self {
        let prefix = [self.prefix, &[name]].concat();

        let mut subcommand = clap::Command::new(name);
        let mut cfg = RoutesCfg {
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

pub trait ConfigureRoutesSet {
    fn set(
        self,
        route: &[&'static str],
        center: &mut WorldCenter,
        command: &mut CommmandMut<'_>,
        router: &mut Router,
    );
}

impl<C> ConfigureRoutesSet for ((), SubRoute<C>)
where
    C: ConfigureRoute,
{
    fn set(
        mut self,
        route: &[&'static str],
        center: &mut WorldCenter,
        command: &mut CommmandMut<'_>,
        router: &mut Router,
    ) {
        let sub_route = [route, &[self.1.command.name]].concat();
        let mut sub_command = CommmandMut(&mut self.1.command.command);

        self.1
            .config
            .one(&sub_route, center, &mut sub_command, router);

        command.mutate(|c| c.subcommand(self.1.command.command));
    }
}

pub struct SubRoute<C: ConfigureRoute> {
    command: CommandInfo,
    config: C,
}

impl<C: ConfigureRoute> SubRoute<C> {
    pub fn new(command: CommandInfo, config: C) -> Self {
        SubRoute { command, config }
    }
}

impl<S, C> ConfigureRoutesSet for (S, SubRoute<C>)
where
    S: ConfigureRoutesSet,
    C: ConfigureRoute,
{
    fn set(
        self,
        route: &[&'static str],
        center: &mut WorldCenter,
        command: &mut CommmandMut<'_>,
        router: &mut Router,
    ) {
        self.0.set(route, center, command, router);
        ((), self.1).set(route, center, command, router);
    }
}

mod sub_routes {
    use clap::{Args, FromArgMatches};
    use lump::prelude::{In, IntoSystem, LumpUnknownError, System};
    use lump_core::world::WorldCenter;

    use crate::router::Router;

    use super::{
        CommandInfo, CommmandMut, ConfigureRoute, ConfigureRoutesSet, Route, SubRoute,
        one_route::OneRoute,
    };

    pub struct SubRoutes<Routes = ()>(Routes);

    impl Default for SubRoutes<()> {
        fn default() -> Self {
            SubRoutes(())
        }
    }

    impl<Routes: ConfigureRoutesSet> ConfigureRoute for SubRoutes<Routes> {
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

    impl<Routes> SubRoutes<Routes> {
        pub fn add<Marker, Arg, S>(
            self,
            route: Route,
            system: S,
        ) -> SubRoutes<impl ConfigureRoutesSet>
        where
            Arg: FromArgMatches + Args + Send + Sync + 'static,
            S: IntoSystem<Marker, System: System<In = In<Arg>, Out = Result<(), LumpUnknownError>>>,
            (Routes, SubRoute<OneRoute<S, (Arg, Marker)>>): ConfigureRoutesSet,
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
            _command: &mut super::CommmandMut<'_>,
            router: &mut crate::router::Router,
        ) {
            let system = make_route_handler(self.system);
            let id = center.register_system(&system);

            router.add(route, id, Box::new(system));
        }
    }
}
