use std::{error::Error, fmt::Display, str::FromStr};

use clap::{CommandFactory, FromArgMatches};
use lump::{
    plugin::Plugin,
    prelude::{In, LumpUnknownError, Res, Resource},
};
use lump_db::{
    Record,
    db::{DbCreate, DbDelete, DbDeleteError, DbList, DbSelectSingle, Query},
};

use crate::schedule::{Route, RoutesCfg};

pub struct RecordCrudCfgBuilder<Marker, Db = (), IdStat = ()> {
    _marker: std::marker::PhantomData<(Marker, Db, IdStat)>,
}

impl<Db, IdStat> RecordCrudCfgBuilder<Db, IdStat> {
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

pub trait RecordCrudCfg {
    type Db: Resource;
    type IdStrategy;
}

impl<Db: Resource, IdStat> RecordCrudCfg for RecordCrudCfgBuilder<Db, IdStat> {
    type Db = Db;
    type IdStrategy = IdStat;
}

pub struct RecordCrudPlugin<Cfg: RecordCrudCfg, R = (), Fc = ()> {
    _marker: std::marker::PhantomData<fn(Cfg, R)>,
    fc: Fc,
}

impl<Cfg: RecordCrudCfg> RecordCrudPlugin<Cfg, (), ()> {
    pub fn new(_cfg: Cfg) -> Self {
        RecordCrudPlugin {
            _marker: std::marker::PhantomData,
            fc: (),
        }
    }

    pub const fn record<R: Record>(
        self,
    ) -> RecordCrudPlugin<Cfg, R, impl FnOnce(&mut RoutesCfg<'_>)> {
        RecordCrudPlugin {
            _marker: std::marker::PhantomData,
            fc: |_: &mut RoutesCfg<'_>| {},
        }
    }
}

impl<Cfg: RecordCrudCfg, R, Fc> RecordCrudPlugin<Cfg, R, Fc>
where
    R: Record,
    Fc: FnOnce(&mut RoutesCfg<'_>),
{
    fn layer(
        self,
        func: impl FnOnce(&mut RoutesCfg<'_>),
    ) -> RecordCrudPlugin<Cfg, R, impl FnOnce(&mut RoutesCfg<'_>)> {
        RecordCrudPlugin {
            _marker: std::marker::PhantomData,
            fc: move |cfg: &mut RoutesCfg<'_>| {
                (self.fc)(cfg);
                func(cfg);
            },
        }
    }

    pub fn list<Item>(self) -> RecordCrudPlugin<Cfg, R, impl FnOnce(&mut RoutesCfg<'_>)>
    where
        Cfg::Db: DbList<Item>,
        Item: Display,
    {
        let system = async |_: In<Unit>, db: Res<'_, Cfg::Db>| {
            let list = db.list(R::TABLE).execute().await?;

            for item in list {
                println!("{}", item);
            }

            Ok(()) as Result<_, LumpUnknownError>
        };

        self.layer(move |cfg| {
            cfg.add("list", system);
        })
    }

    pub fn create<Data>(self) -> RecordCrudPlugin<Cfg, R, impl FnOnce(&mut RoutesCfg<'_>)>
    where
        Cfg::Db: DbCreate<R, Data>,
        Data: Send + Sync + CommandFactory + FromArgMatches + 'static,
        R: Display,
    {
        let system = async |data: In<Data>, db: Res<'_, Cfg::Db>| {
            let id = db.create(R::TABLE, data.0).execute().await?;
            println!("Created `{}`:{}", R::TABLE, id);

            Ok(())
        };

        self.layer(move |cfg| {
            cfg.add("create", system);
        })
    }

    pub fn delete(self) -> RecordCrudPlugin<Cfg, R, impl FnOnce(&mut RoutesCfg<'_>)>
    where
        Cfg::Db: DbDelete<R>,
        R: FromStr<Err: Error> + Display,
        R::Err: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        let system = async |id: In<ArgsId<R>>, db: Res<'_, Cfg::Db>| {
            let result = db.delete(R::TABLE, id.0.id).execute().await?;

            if let Err(DbDeleteError::None) = result {
                println!("Could not find `{}`:{}", R::TABLE, id.0.id);
                return Ok(());
            }

            println!("Deleted `{}`:{}", R::TABLE, id.0.id);
            Ok(())
        };

        self.layer(move |cfg| {
            cfg.add("delete", system);
        })
    }

    pub fn get<Data>(self) -> RecordCrudPlugin<Cfg, R, impl FnOnce(&mut RoutesCfg<'_>)>
    where
        Cfg::Db: DbSelectSingle<R, Data>,
        Data: Display,
        R: FromStr<Err: Error> + Display,
        R::Err: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        let system = async |id: In<ArgsId<R>>, db: Res<'_, Cfg::Db>| {
            let result = db.select(R::TABLE, id.0.id).execute().await?;

            let Some(data) = result else {
                println!("Could not find `{}`:{}", R::TABLE, id.0.id);
                return Ok(());
            };

            println!("{}", data);

            Ok(())
        };

        self.layer(move |cfg| {
            cfg.add("get", system);
        })
    }

    pub fn all<Data>(self) -> RecordCrudPlugin<Cfg, R, impl FnOnce(&mut RoutesCfg<'_>)>
    where
        Cfg::Db: DbList<Data> + DbSelectSingle<R, Data> + DbDelete<R> + DbCreate<R, Data>,
        R: FromStr<Err: Error> + Display,
        R::Err: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
        Data: Display + Send + Sync + CommandFactory + FromArgMatches + 'static,
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

impl<Cfg: RecordCrudCfg, R, Fc> Plugin for RecordCrudPlugin<Cfg, R, Fc>
where
    R: Record,
    Fc: FnOnce(&mut RoutesCfg<'_>),
{
    fn setup(self, app: impl lump::prelude::ConfigureApp) {
        app.add_system(Route(R::TABLE), self.fc);
    }
}

