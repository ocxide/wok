use std::{error::Error, fmt::Display, str::FromStr};

use lump::{
    plugin::Plugin,
    prelude::{In, Res, Resource},
};
use lump_db::{
    Record,
    db::{DbCreate, DbDelete, DbDeleteError, DbList, DbSelectSingle, Query},
    id_strategy::IdStrategy,
    RecordEntry,
};

use crate::schedule::{ConfigureRoute, ConfigureRoutesSet, Route, SubRoutes, cardinality};

pub struct RecordCrudCfgBuilder<Db = (), IdStat = ()> {
    _marker: std::marker::PhantomData<(Db, IdStat)>,
}

impl<Db, IdStat> RecordCrudCfgBuilder<Db, IdStat> {
    pub const fn new() -> RecordCrudCfgBuilder<Db, IdStat> {
        RecordCrudCfgBuilder {
            _marker: std::marker::PhantomData,
        }
    }

    pub const fn db<Db2: Resource>(self) -> RecordCrudCfgBuilder<Db2, IdStat> {
        RecordCrudCfgBuilder {
            _marker: std::marker::PhantomData,
        }
    }

    pub const fn id<IdStat2>(self) -> RecordCrudCfgBuilder<Db, IdStat2> {
        RecordCrudCfgBuilder {
            _marker: std::marker::PhantomData,
        }
    }
}

impl Default for RecordCrudCfgBuilder<(), ()> {
    fn default() -> Self {
        Self::new()
    }
}

pub trait RecordCrudCfg {
    type Db: Resource;
    type IdStrategy;
}

impl<Db: Resource, IdStat> RecordCrudCfg for RecordCrudCfgBuilder<Db, IdStat> {
    type Db = Db;
    type IdStrategy = IdStat;
}

pub struct RecordCrudPlugin<Cfg: RecordCrudCfg, R = (), Routes = ()> {
    _marker: std::marker::PhantomData<fn(Cfg, R)>,
    subroutes: Routes,
}

impl<Cfg: RecordCrudCfg> RecordCrudPlugin<Cfg, (), ()> {
    pub fn new(_cfg: Cfg) -> Self {
        RecordCrudPlugin {
            _marker: std::marker::PhantomData,
            subroutes: (),
        }
    }

    pub const fn record<R: Record>(self) -> RecordCrudPlugin<Cfg, R, SubRoutes> {
        RecordCrudPlugin {
            _marker: std::marker::PhantomData,
            subroutes: SubRoutes::empty(),
        }
    }
}

impl<Cfg: RecordCrudCfg, R, Routes> RecordCrudPlugin<Cfg, R, SubRoutes<Routes>>
where
    R: Record,
    Routes: ConfigureRoutesSet,
{
    pub fn list<Item>(
        self,
    ) -> RecordCrudPlugin<
        Cfg,
        R,
        SubRoutes<impl ConfigureRoutesSet<Cardinality = cardinality::OneOrMore>>,
    >
    where
        R: Display,
        Cfg::Db: DbList<RecordEntry<R, Item>>,
        Item: Display,
    {
        let system = async |_: In<Unit>, db: Res<'_, Cfg::Db>| {
            let list = db.list(R::TABLE).execute().await?;

            if list.is_empty() {
                println!("<None>");
            }
            for entry in list {
                println!("{}:{} {}", R::TABLE, entry.id, entry.data);
            }

            Ok(())
        };

        RecordCrudPlugin {
            _marker: std::marker::PhantomData,
            subroutes: self.subroutes.add(Route("list"), system),
        }
    }

    pub fn create<Data>(
        self,
    ) -> RecordCrudPlugin<
        Cfg,
        R,
        SubRoutes<impl ConfigureRoutesSet<Cardinality = cardinality::OneOrMore>>,
    >
    where
        Cfg::Db: DbCreate<R, <Cfg::IdStrategy as IdStrategy<R>>::Wrap<Data>>,
        Cfg::IdStrategy: IdStrategy<R>,
        Data: Send + Sync + clap::Args + clap::FromArgMatches + 'static,
        R: Display,
    {
        let system = async |data: In<Data>, db: Res<'_, Cfg::Db>| {
            let body = <Cfg::IdStrategy as IdStrategy<R>>::wrap(data.0);
            let id = db.create(R::TABLE, body).execute().await?;
            println!("Created {}:{}", R::TABLE, id);

            Ok(())
        };

        RecordCrudPlugin {
            _marker: std::marker::PhantomData,
            subroutes: self.subroutes.add(Route("create"), system),
        }
    }

    pub fn delete(
        self,
    ) -> RecordCrudPlugin<
        Cfg,
        R,
        SubRoutes<impl ConfigureRoutesSet<Cardinality = cardinality::OneOrMore>>,
    >
    where
        Cfg::Db: DbDelete<R>,
        R: FromStr<Err: Error> + Display,
        R::Err: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        let system = async |id: In<ArgsId<R>>, db: Res<'_, Cfg::Db>| {
            let result = db.delete(R::TABLE, id.0.id).execute().await?;

            if let Err(DbDeleteError::None) = result {
                println!("Could not find {}:{}", R::TABLE, id.0.id);
                return Ok(());
            }

            println!("Deleted `{}`:{}", R::TABLE, id.0.id);
            Ok(())
        };

        RecordCrudPlugin {
            _marker: std::marker::PhantomData,
            subroutes: self.subroutes.add(Route("delete"), system),
        }
    }

    pub fn get<Data>(
        self,
    ) -> RecordCrudPlugin<
        Cfg,
        R,
        SubRoutes<impl ConfigureRoutesSet<Cardinality = cardinality::OneOrMore>>,
    >
    where
        Cfg::Db: DbSelectSingle<R, Data>,
        Data: Display,
        R: FromStr<Err: Error> + Display,
        R::Err: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        let system = async |id: In<ArgsId<R>>, db: Res<'_, Cfg::Db>| {
            let result = db.select(R::TABLE, id.0.id).execute().await?;

            let Some(data) = result else {
                println!("Could not find {}:{}", R::TABLE, id.0.id);
                return Ok(());
            };

            println!("{}:{} {}", R::TABLE, id.0.id, data);

            Ok(())
        };

        RecordCrudPlugin {
            _marker: std::marker::PhantomData,
            subroutes: self.subroutes.add(Route("get"), system),
        }
    }

    pub fn all<Data>(
        self,
    ) -> RecordCrudPlugin<
        Cfg,
        R,
        SubRoutes<impl ConfigureRoutesSet<Cardinality = cardinality::OneOrMore>>,
    >
    where
        Cfg::IdStrategy: IdStrategy<R>,
        Cfg::Db: DbList<RecordEntry<R, Data>>
            + DbSelectSingle<R, Data>
            + DbDelete<R>
            + DbCreate<R, <Cfg::IdStrategy as IdStrategy<R>>::Wrap<Data>>,
        R: FromStr<Err: Error> + Display,
        R::Err: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
        Data: Display + Send + Sync + clap::Args + clap::FromArgMatches + 'static,
    {
        self.list().create().delete().get()
    }
}

#[derive(clap::Parser)]
struct Unit;

#[derive(clap::Parser)]
struct ArgsId<
    R: FromStr<Err: Error + Into<Box<dyn std::error::Error + Send + Sync + 'static>>>
        + Sync
        + Send
        + Clone
        + 'static,
> {
    id: R,
}

impl<Cfg: RecordCrudCfg, R, SubRoutes> Plugin for RecordCrudPlugin<Cfg, R, SubRoutes>
where
    R: Record,
    SubRoutes: ConfigureRoute,
{
    fn setup(self, app: impl lump::prelude::ConfigureApp) {
        app.add_system(Route(R::TABLE), self.subroutes);
    }
}
