use dust::{
    dust::{ConfigureDust, Dust},
    prelude::{DynSystem, IntoSystem, System},
};
use futures::StreamExt;

mod router {
    use std::collections::HashMap;

    use dust::prelude::Resource;
    use dust_db::Record;

    use crate::record_systems::{ClapRecordSystems, SubCommandSystems};

    pub struct RouterBuilder<Db> {
        _phantom: std::marker::PhantomData<fn(Db)>,
        systems: HashMap<&'static str, SubCommandSystems>,
        command: clap::Command,
    }

    impl<Db: Resource> Default for RouterBuilder<Db> {
        fn default() -> Self {
            Self {
                _phantom: std::marker::PhantomData,
                systems: Default::default(),
                command: Default::default(),
            }
        }
    }

    impl<Db: Resource> RouterBuilder<Db> {
        pub fn by_record<R: Record>(
            mut self,
            f: impl FnOnce(ClapRecordSystems<Db, R>) -> ClapRecordSystems<Db, R>,
        ) -> Self {
            let systems = f(ClapRecordSystems::<Db, R>::default());

            let (subcommand, systems) = systems.build();
            self.systems.insert(R::TABLE, systems);

            take_mut::take(&mut self.command, |c| c.subcommand(subcommand));

            self
        }

        pub fn build(self) -> Router {
            Router {
                systems: self.systems,
                command: self.command,
            }
        }
    }

    pub struct Router {
        pub systems: HashMap<&'static str, SubCommandSystems>,
        pub command: clap::Command,
    }

    impl Router {
        #[inline]
        pub fn get(&self, name: &str) -> Option<&SubCommandSystems> {
            self.systems.get(name)
        }
    }
}

mod record_systems {
    use std::{collections::HashMap, fmt::Display};

    use clap::{ArgMatches, Args, FromArgMatches};
    use dust::prelude::{DynSystem, In, IntoSystem, Res, Resource, System};
    use dust_db::{
        Record,
        db::{DbList, DbOwnedCreate, Query},
    };

    type DynClapSystem = DynSystem<ArgMatches, ()>;

    #[derive(Default)]
    pub struct SubCommandSystems(HashMap<&'static str, DynClapSystem>);

    impl SubCommandSystems {
        #[inline]
        pub fn get(&self, name: &str) -> Option<&DynClapSystem> {
            self.0.get(name)
        }
    }

    pub struct ClapRecordSystems<Db, R> {
        command: clap::Command,
        systems: SubCommandSystems,
        _phantom: std::marker::PhantomData<fn(Db, R)>,
    }

    impl<Db: Resource, R: Record> Default for ClapRecordSystems<Db, R> {
        fn default() -> Self {
            let command = clap::Command::new(R::TABLE);

            Self {
                command,
                systems: Default::default(),
                _phantom: std::marker::PhantomData,
            }
        }
    }

    impl<Db: Resource, R: Record> ClapRecordSystems<Db, R> {
        pub fn create_by<D>(mut self) -> Self
        where
            D: Args + 'static,
            Db: DbOwnedCreate<D>,
        {
            const COMMAND_NAME: &str = "create";

            async fn create_system<Db, D, R>(args: In<ArgMatches>, db: Res<'_, Db>)
            where
                Db: Resource + DbOwnedCreate<D>,
                D: FromArgMatches,
                R: Record,
            {
                let data = D::from_arg_matches(&args).unwrap();
                db.create(R::TABLE, data).execute().await.unwrap();
            }

            self.add(COMMAND_NAME, |c| c.alias("c"), create_system::<Db, D, R>);
            self
        }

        pub fn list_by<D>(mut self) -> Self
        where
            Db: DbList<D>,
            D: Display + 'static,
        {
            async fn list_system<Db, D, R>(_: In<ArgMatches>, db: Res<'_, Db>)
            where
                Db: Resource + DbList<D>,
                D: Display,
                R: Record,
            {
                let items = db.list(R::TABLE).execute().await.unwrap();

                for item in items {
                    println!("{}", item);
                }
            }

            self.add("list", |c| c.alias("ls"), list_system::<Db, D, R>);
            self
        }

        fn add<M>(
            &mut self,
            command_name: &'static str,
            command_factory: impl FnOnce(clap::Command) -> clap::Command,
            system: impl IntoSystem<M, System: System<In = ArgMatches, Out = ()>>,
        ) {
            let subcommand = command_factory(clap::Command::new(command_name));
            take_mut::take(&mut self.command, |command| command.subcommand(subcommand));

            self.systems
                .0
                .insert(command_name, Box::new(system.into_system()));
        }

        pub fn build(self) -> (clap::Command, SubCommandSystems) {
            (
                self.command
                    .subcommand_required(true)
                    .arg_required_else_help(true),
                self.systems,
            )
        }
    }
}

use router::Router;
pub use router::RouterBuilder;

#[derive(Default)]
pub struct App {
    dust: dust::prelude::Dust,
    startup_systems: Vec<DynSystem<(), ()>>,
}

impl ConfigureDust for App {
    fn dust(&mut self) -> &mut Dust {
        &mut self.dust
    }
}

impl App {
    pub fn add_startup_system<S, Marker>(mut self, system: S) -> Self
    where
        S: IntoSystem<Marker, System: System<In = (), Out = ()>>,
    {
        self.startup_systems.push(Box::new(system.into_system()));
        self
    }

    pub fn run(mut self, mut router: Router) -> impl Future<Output = ()> {
        use futures::stream::FuturesUnordered;

        router.command.build();
        let matches = router
            .command
            .subcommand_required(true)
            .arg_required_else_help(true)
            .get_matches()
            .remove_subcommand();

        async move {
            let Some((command_name, mut args)) = matches else {
                return;
            };

            let systems = router.systems.get(command_name.as_str()).unwrap();

            let Some((command_name, args)) = args.remove_subcommand() else {
                return;
            };

            let fut = self
                .startup_systems
                .iter()
                .map(|system| system.run(&self.dust, ()))
                .collect::<FuturesUnordered<_>>();

            let _ = fut.into_future().await;
            self.dust.tick_commands();

            let system = systems.get(&command_name).unwrap();
            system.run(&self.dust, args).await;
        }
    }
}
