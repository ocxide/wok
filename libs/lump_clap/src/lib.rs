use lump::schedules::Startup;
use lump_core::world::{ConfigureWorld, World};

pub mod prelude {
    pub use crate::App;
    pub use lump::prelude::*;
}

mod router {
    use std::collections::HashMap;

    use lump::prelude::Resource;
    use lump_db::Record;

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

    pub trait RouterConfig: 'static {
        type Db: Resource;
        type IdStrat;
    }

    impl<Db: Resource, IdStrat: 'static> RouterConfig for RouterCfg<Db, IdStrat> {
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
    use std::{collections::HashMap, fmt::Display, str::FromStr};

    use clap::{ArgMatches, Args, FromArgMatches};
    use lump::prelude::{DynSystem, In, IntoSystem, LumpUnknownError, Res, Resource, TaskSystem};
    use lump_db::{
        Record,
        db::{DbDelete, DbDeleteError, DbList, DbOwnedCreate, DbSelectSingle, IdStrategy, Query},
    };

    use crate::router::RouterConfig;

    type DynClapSystem = DynSystem<ArgMatches, Result<(), LumpUnknownError>>;

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

    async fn delete_system_inner<Db, R>(id: R, db: Res<'_, Db>) -> Result<(), LumpUnknownError>
    where
        Db: Resource + DbDelete<R>,
        R: Record + Display,
    {
        let result = db.delete(R::TABLE, id).execute().await?;
        match result {
            Ok(()) => {
                println!("Deleted {}:{}", R::TABLE, id);
            }
            Err(DbDeleteError::None) => {
                eprintln!("Failed to delete {}:{}; Not found", R::TABLE, id);
            }
        }

        Ok(())
    }

    impl<Config: RouterConfig, R: Record> ClapRecordSystems<Config, R> {
        pub fn create_by<D>(mut self) -> Self
        where
            D: Args + 'static,
            Config::Db: DbOwnedCreate<R, <Config::IdStrat as IdStrategy<R>>::Wrap<D>>,
            Config::IdStrat: IdStrategy<R>,
            R: Display,
        {
            const COMMAND_NAME: &str = "create";

            async fn create_system<Db, IdStrat, D, R>(
                args: In<ArgMatches>,
                db: Res<'_, Db>,
            ) -> Result<(), LumpUnknownError>
            where
                Db: Resource + DbOwnedCreate<R, IdStrat::Wrap<D>>,
                IdStrat: IdStrategy<R>,
                D: FromArgMatches,
                R: Record + Display,
            {
                let data = D::from_arg_matches(&args).expect("failed to parse data");
                let data = IdStrat::wrap(data);

                let id = db.create(R::TABLE, data).execute().await?;

                println!("Created {}:{}", R::TABLE, id);
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
            Config::Db: DbList<lump_db::data_wrappers::KeyValue<R, D>>,
            D: Display + 'static,
            R: Display,
        {
            async fn list_system<Db, D, R>(
                _: In<ArgMatches>,
                db: Res<'_, Db>,
            ) -> Result<(), LumpUnknownError>
            where
                Db: Resource + DbList<lump_db::data_wrappers::KeyValue<R, D>>,
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

        pub fn delete_by_alias<A>(mut self) -> Self
        where
            Config::Db: DbDelete<R> + DbSelectSingle<R, A>,
            R: FromStr<Err: std::error::Error> + Display,
            A: FromStr<Err: std::error::Error> + 'static,
        {
            async fn alias_delete<Config: RouterConfig, R, A>(
                args: In<ArgMatches>,
                db: Res<'_, Config::Db>,
            ) -> Result<(), LumpUnknownError>
            where
                Config::Db: DbDelete<R> + DbSelectSingle<R, A>,
                R: FromStr<Err: std::error::Error> + Display + Record,
                A: FromStr<Err: std::error::Error>,
            {
                let id_str = args.get_one::<String>("id").expect("failed to get id");
                let id: R;
                if let Some(id_str) = id_str.strip_prefix('#') {
                    id = match R::from_str(id_str) {
                        Ok(id) => id,
                        Err(e) => {
                            eprintln!("Failed to parse id: {}", e);
                            return Ok(());
                        }
                    };
                } else {
                    let alias = match A::from_str(id_str) {
                        Ok(alias) => alias,
                        Err(e) => {
                            eprintln!("Failed to parse alias: {}", e);
                            return Ok(());
                        }
                    };

                    let id_res = db.select(R::TABLE, alias).execute().await?;
                    match id_res {
                        Some(found_id) => id = found_id,
                        None => {
                            eprintln!("Failed to find by alias");
                            return Ok(());
                        }
                    };
                }

                delete_system_inner(id, db).await
            }

            self.add(
                "delete",
                |c| c.alias("d").arg(clap::Arg::new("id").required(true)),
                alias_delete::<Config, R, A>,
            );

            self
        }

        pub fn delete(mut self) -> Self
        where
            Config::Db: DbDelete<R>,
            R: FromStr<Err: std::error::Error> + Display,
        {
            const COMMAND_NAME: &str = "delete";

            async fn delete<Config: RouterConfig, R>(
                args: In<ArgMatches>,
                db: Res<'_, Config::Db>,
            ) -> Result<(), LumpUnknownError>
            where
                Config::Db: DbDelete<R>,
                R: Record + FromStr<Err: std::error::Error> + Display,
            {
                let id = args.get_one::<String>("id").expect("failed to get id");
                let id = match R::from_str(id) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Failed to parse id: {}", e);
                        return Ok(());
                    }
                };

                delete_system_inner(id, db).await
            }

            self.add(
                COMMAND_NAME,
                |c| c.alias("d").arg(clap::Arg::new("id").required(true)),
                delete::<Config, R>,
            );

            self
        }

        fn add<M>(
            &mut self,
            command_name: &'static str,
            command_factory: impl FnOnce(clap::Command) -> clap::Command,
            system: impl IntoSystem<
                M,
                System: TaskSystem<In = ArgMatches, Out = Result<(), LumpUnknownError>>,
            >,
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

pub struct App {
    lump: lump::prelude::World,
}

impl Default for App {
    fn default() -> Self {
        let mut lump = lump::prelude::World::default();
        lump.init_schedule::<Startup>();

        Self { lump }
    }
}

impl ConfigureWorld for App {
    fn world_mut(&mut self) -> &mut World {
        &mut self.lump
    }

    fn world(&self) -> &World {
        &self.lump
    }
}

impl App {
    pub fn run(self, mut router: Router) -> impl Future<Output = ()> {
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

            let system = systems
                .get(&command_name)
                .expect("clap unmatched subcommand");

            let result = system.run(&self.lump.state, args).await;
            if let Err(err) = result {
                eprintln!("ERROR: {}", err);
            }
        }
    }
}
