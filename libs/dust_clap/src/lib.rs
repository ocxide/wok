use dust::{
    dust::{ConfigureDust, Dust},
    error::DustUnknownError,
    prelude::{DynSystem, IntoSystem, System},
};
use futures::StreamExt;

mod router {
    use std::collections::HashMap;

    use dust::prelude::Resource;
    use dust_db::Record;

    use crate::record_systems::{ClapRecordSystems, ClapSubSystems, SubCommandSystems};

    pub struct RouterCfg<Db = (), IdStrat = ()> {
        _phantom: std::marker::PhantomData<fn(Db, IdStrat)>,
    }

    impl RouterCfg<(), ()> {
        #[inline]
        pub const fn new() -> RouterCfg {
            RouterCfg {
                _phantom: std::marker::PhantomData,
            }
        }
    }

    impl Default for RouterCfg<(), ()> {
        #[inline]
        fn default() -> Self {
            Self::new()
        }
    }

    impl<Db, IdStrat> RouterCfg<Db, IdStrat> {
        #[inline]
        pub const fn use_db<Db2>(self) -> RouterCfg<Db2, IdStrat> {
            RouterCfg {
                _phantom: std::marker::PhantomData,
            }
        }

        #[inline]
        pub const fn use_id_strat<IdStrat2>(self) -> RouterCfg<Db, IdStrat2> {
            RouterCfg {
                _phantom: std::marker::PhantomData,
            }
        }
    }

    pub trait RouterConfig {
        type Db: Resource;
        type IdStrat;
    }

    impl<Db: Resource, IdStrat> RouterConfig for RouterCfg<Db, IdStrat> {
        type Db = Db;
        type IdStrat = IdStrat;
    }

    pub struct RouterBuilder<Config = ()> {
        _phantom: std::marker::PhantomData<fn(Config)>,
        router: Router,
    }

    impl RouterBuilder<()> {
        #[inline]
        pub fn new<Config>(_config: Config) -> RouterBuilder<Config> {
            RouterBuilder {
                _phantom: std::marker::PhantomData,
                router: Router::default(),
            }
        }
    }

    impl<Config: RouterConfig> RouterBuilder<Config> {
        pub fn by_record<R: Record>(
            mut self,
            f: impl FnOnce(ClapRecordSystems<Config, R>) -> ClapSubSystems,
        ) -> Self {
            let subsystems = f(ClapRecordSystems::default());

            self.router.systems.insert(R::TABLE, subsystems.systems);

            take_mut::take(&mut self.router.command, |c| {
                c.subcommand(subsystems.command)
            });

            self
        }

        #[inline]
        pub fn build(self) -> Router {
            self.router
        }
    }

    #[derive(Default)]
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
    use dust::{error::DustUnknownError, prelude::{DynSystem, In, IntoSystem, Res, Resource, System}};
    use dust_db::{
        Record,
        db::{DbList, DbOwnedCreate, IdStrategy, Query},
    };

    use crate::router::RouterConfig;

    type DynClapSystem = DynSystem<ArgMatches, Result<(), DustUnknownError>>;

    #[derive(Default)]
    pub struct SubCommandSystems(HashMap<&'static str, DynClapSystem>);

    impl SubCommandSystems {
        #[inline]
        pub fn get(&self, name: &str) -> Option<&DynClapSystem> {
            self.0.get(name)
        }
    }

    pub struct ClapSubSystems {
        pub command: clap::Command,
        pub systems: SubCommandSystems,
    }

    pub struct ClapRecordSystems<Config, R> {
        subsystems: ClapSubSystems,
        _phantom: std::marker::PhantomData<fn(Config, R)>,
    }

    impl<Config: RouterConfig, R: Record> Default for ClapRecordSystems<Config, R> {
        fn default() -> Self {
            let command = clap::Command::new(R::TABLE);

            Self {
                subsystems: ClapSubSystems {
                    command,
                    systems: SubCommandSystems::default(),
                },
                _phantom: std::marker::PhantomData,
            }
        }
    }

    impl<Config: RouterConfig, R: Record> ClapRecordSystems<Config, R> {
        pub fn create_by<D>(mut self) -> Self
        where
            D: Args + 'static,
            Config::Db: DbOwnedCreate<<Config::IdStrat as IdStrategy<R>>::Wrap<D>>,
            Config::IdStrat: IdStrategy<R>,
        {
            const COMMAND_NAME: &str = "create";

            async fn create_system<Db, IdStrat, D, R>(args: In<ArgMatches>, db: Res<'_, Db>) -> Result<(), DustUnknownError>
            where
                Db: Resource + DbOwnedCreate<IdStrat::Wrap<D>>,
                IdStrat: IdStrategy<R>,
                D: FromArgMatches,
                R: Record,
            {
                let data = D::from_arg_matches(&args).expect("failed to parse data");
                let data = IdStrat::wrap(data);

                db.create(R::TABLE, data).execute().await?;

                println!("Created {}", R::TABLE);
                Ok(())
            }

            self.add(
                COMMAND_NAME,
                |c| {
                    let command = c.alias("c");
                    D::augment_args(command)
                },
                create_system::<Config::Db, Config::IdStrat, D, R>,
            );
            self
        }

        pub fn list_by<D>(mut self) -> Self
        where
            Config::Db: DbList<dust_db::data_wrappers::KeyValue<R, D>>,
            D: Display + 'static,
            R: Display,
        {
            async fn list_system<Db, D, R>(_: In<ArgMatches>, db: Res<'_, Db>) -> Result<(), DustUnknownError>
            where
                Db: Resource + DbList<dust_db::data_wrappers::KeyValue<R, D>>,
                D: Display,
                R: Record + Display,
            {
                let items = db.list(R::TABLE).execute().await?;

                if items.is_empty() {
                    println!("<None>");
                    return Ok(());
                }

                for kv in items {
                    println!("#({}): {}", kv.id, kv.data);
                }

                Ok(())
            }

            self.add("list", |c| c.alias("ls"), list_system::<Config::Db, D, R>);
            self
        }

        fn add<M>(
            &mut self,
            command_name: &'static str,
            command_factory: impl FnOnce(clap::Command) -> clap::Command,
            system: impl IntoSystem<M, System: System<In = ArgMatches, Out = Result<(), DustUnknownError>>>,
        ) {
            let subcommand = command_factory(clap::Command::new(command_name));
            take_mut::take(&mut self.subsystems.command, |command| {
                command.subcommand(subcommand)
            });

            self.subsystems
                .systems
                .0
                .insert(command_name, Box::new(system.into_system()));
        }

        #[inline]
        pub fn build(self) -> ClapSubSystems {
            self.subsystems
        }
    }
}

use router::Router;
pub use router::{RouterBuilder, RouterCfg};

#[derive(Default)]
pub struct App {
    dust: dust::prelude::Dust,
    startup_systems: Vec<DynSystem<(), Result<(), DustUnknownError>>>,
}

impl ConfigureDust for App {
    fn dust(&mut self) -> &mut Dust {
        &mut self.dust
    }
}

impl App {
    pub fn add_startup_system<S, Marker>(mut self, system: S) -> Self
    where
        S: IntoSystem<Marker, System: System<In = (), Out = Result<(), DustUnknownError>>>,
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

            let systems = router
                .systems
                .get(command_name.as_str())
                .expect("clap unmatched command");

            let Some((command_name, args)) = args.remove_subcommand() else {
                return;
            };

            let mut fut = self
                .startup_systems
                .iter()
                .map(|system| system.run(&self.dust, ()))
                .collect::<FuturesUnordered<_>>();

            loop {
                let (out, fut_) = fut.into_future().await;
                fut = fut_;

                match out {
                    Some(Ok(_)) => {}
                    Some(Err(err)) => panic!("Startup failed: {}", err),
                    None => break,
                }
            }

            self.dust.tick_commands();

            let system = systems
                .get(&command_name)
                .expect("clap unmatched subcommand");

            let result = system.run(&self.dust, args).await;
            if let Err(err) = result {
                eprintln!("ERROR: {}", err);
            }
        }
    }
}
