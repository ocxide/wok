use std::{collections::HashMap, fmt::Display};

use clap::{ArgMatches, Args, FromArgMatches};
use dust::{Resource, prelude::*};
use dust_db::{
    Record,
    db::{DbList, DbOwnedCreate, Query},
};

type DynClapSystem = Box<dyn System<In = ArgMatches, Out = ()>>;

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
        (self.command, self.systems)
    }
}
