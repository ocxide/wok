use std::borrow::Cow;

use axum::Json;
use wok::{
    plugin::Plugin,
    prelude::{ConfigureWorld, In, Res, Resource, WokUnknownError},
};
use wok_db::{
    Record,
    db::{DbCreate, DbDelete, DbList, DbSelectSingle, Query},
    id_strategy::IdStrategy,
};

use crate::{Route, delete, get, post};

pub struct CRUDCfgBuilder<Db = (), IdStrategy = ()>(std::marker::PhantomData<(Db, IdStrategy)>);

impl<Db> Clone for CRUDCfgBuilder<Db> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Db> Copy for CRUDCfgBuilder<Db> {}

impl CRUDCfgBuilder<()> {
    pub const fn new() -> CRUDCfgBuilder<()> {
        CRUDCfgBuilder(std::marker::PhantomData)
    }
}

impl<Db, IdStrategy> CRUDCfgBuilder<Db, IdStrategy> {
    pub const fn db<Db2: Resource>(self) -> CRUDCfgBuilder<Db2> {
        CRUDCfgBuilder(std::marker::PhantomData)
    }

    pub const fn id<IdStrategy2>(self) -> CRUDCfgBuilder<Db, IdStrategy2> {
        CRUDCfgBuilder(std::marker::PhantomData)
    }
}

impl Default for CRUDCfgBuilder<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Db: Resource, IdStrategy> CrudConfig for CRUDCfgBuilder<Db, IdStrategy> {
    type Db = Db;
    type IdStrategy = IdStrategy;
}

pub trait CrudConfig: Sized {
    type Db: Resource;
    type IdStrategy;

    fn for_record<R: Record>(self) -> RoutePluginBuilder<R, Self> {
        RoutePluginBuilder {
            path: format!("/{}", R::TABLE).into(),
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct RoutePluginBuilder<R: Record, Config: CrudConfig> {
    pub path: Cow<'static, str>,
    _marker: std::marker::PhantomData<fn(R, Config)>,
}

impl<R: Record, Config: CrudConfig> RoutePluginBuilder<R, Config> {
    pub fn with_path(path: &'static str) -> Self {
        Self {
            path: path.into(),
            _marker: std::marker::PhantomData,
        }
    }

    pub const fn list_all<D: serde::Serialize>(&self) -> ListAllPlugin<'_, R, D, Config>
    where
        Config::Db: DbList<wok_db::RecordEntry<R, D>>,
    {
        ListAllPlugin {
            builder: self,
            _marker: std::marker::PhantomData,
        }
    }

    pub const fn create_one<D: valigate::Valid>(&self) -> CreateOnePlugin<'_, R, D, Config>
    where
        Config::Db: DbCreate<R, D>,
        D::In: serde::de::DeserializeOwned,
    {
        CreateOnePlugin {
            builder: self,
            _marker: std::marker::PhantomData,
        }
    }

    pub const fn delete_one(&self) -> DeleteOnePlugin<'_, R, Config>
    where
        Config::Db: DbDelete<R>,
    {
        DeleteOnePlugin {
            builder: self,
            _marker: std::marker::PhantomData,
        }
    }

    pub const fn get_one<D>(&self) -> GetOnePlugin<'_, R, D, Config>
    where
        D: serde::Serialize,
        Config::Db: DbSelectSingle<R, wok_db::RecordEntry<R, D>>,
    {
        GetOnePlugin {
            builder: self,
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct ListAllPlugin<'b, R: Record, D: serde::Serialize, Config: CrudConfig>
where
    Config::Db: DbList<wok_db::RecordEntry<R, D>>,
{
    builder: &'b RoutePluginBuilder<R, Config>,
    _marker: std::marker::PhantomData<fn(D)>,
}

impl<'b, R: Record, D: serde::Serialize + Send + Sync + 'static, Config: CrudConfig> Plugin
    for ListAllPlugin<'b, R, D, Config>
where
    R: serde::Serialize,
    Config::Db: DbList<wok_db::RecordEntry<R, D>>,
{
    fn setup(self, app: &mut wok::prelude::App) {
        let system = async move |db: Res<'_, Config::Db>| -> Result<
            axum::Json<Vec<wok_db::RecordEntry<R, D>>>,
            WokUnknownError,
        > {
            let data = db.list(R::TABLE).execute().await?;
            Ok(axum::Json(data))
        };

        app.add_systems(Route(&self.builder.path), get(system));
    }
}

pub struct CreateOnePlugin<
    'b,
    R: Record,
    D: valigate::Valid<In: serde::de::DeserializeOwned>,
    Config: CrudConfig,
> {
    builder: &'b RoutePluginBuilder<R, Config>,
    _marker: std::marker::PhantomData<fn(R, D)>,
}

type Wrap<IdStrat, R, D> = <IdStrat as IdStrategy<R>>::Wrap<D>;

impl<'b, R: Record, D, Config: CrudConfig> Plugin for CreateOnePlugin<'b, R, D, Config>
where
    D: valigate::Valid + Send + Sync + 'static,
    D::In: serde::de::DeserializeOwned + Send + Sync + 'static,
    Config::Db: DbCreate<R, Wrap<Config::IdStrategy, R, D>>,
    Config::IdStrategy: IdStrategy<R>,
    R: serde::de::DeserializeOwned + serde::Serialize,
{
    fn setup(self, app: &mut wok::prelude::App) {
        use crate::{extract::JsonG, response::Created};

        let system = async move |In(JsonG(data)): In<JsonG<D>>, db: Res<'_, Config::Db>| {
            let data = Config::IdStrategy::wrap(data);
            let id = db.create(R::TABLE, data).execute().await?;

            Ok(Created::Created(Json(id))) as Result<_, WokUnknownError>
        };

        app.add_systems(Route(&self.builder.path), post(system));
    }
}

pub struct GetOnePlugin<'b, R: Record, D: serde::Serialize, Config: CrudConfig> {
    builder: &'b RoutePluginBuilder<R, Config>,
    _marker: std::marker::PhantomData<fn(D)>,
}

impl<'b, R: Record, D: serde::Serialize, Config: CrudConfig> Plugin
    for GetOnePlugin<'b, R, D, Config>
where
    R: serde::de::DeserializeOwned + serde::Serialize,
    D: Send + Sync + 'static,
    Config::Db: DbSelectSingle<R, wok_db::RecordEntry<R, D>>,
{
    fn setup(self, app: &mut wok::prelude::App) {
        let system = async move |In(axum::extract::Path(id)): In<axum::extract::Path<R>>,
                                 db: Res<'_, Config::Db>| {
            let data = db.select(R::TABLE, id).execute().await?;
            (match data {
                Some(data) => Ok(Ok(axum::Json(data))),
                None => Ok(Err(axum::http::StatusCode::NOT_FOUND)),
            }) as Result<_, WokUnknownError>
        };

        app.add_systems(Route(&format!("{}/{{id}}", self.builder.path)), get(system));
    }
}

pub struct DeleteOnePlugin<'b, R: Record, Config: CrudConfig> {
    builder: &'b RoutePluginBuilder<R, Config>,
    _marker: std::marker::PhantomData<fn(R)>,
}

impl<'b, R: Record, Config: CrudConfig> Plugin for DeleteOnePlugin<'b, R, Config>
where
    R: serde::de::DeserializeOwned,
    Config::Db: DbDelete<R>,
{
    fn setup(self, app: &mut wok::prelude::App) {
        let system = async move |In(axum::extract::Path(id)): In<axum::extract::Path<R>>,
                                 db: Res<'_, Config::Db>| {
            let result = db.delete(R::TABLE, id).execute().await?;
            (match result {
                Ok(()) => Ok(axum::http::StatusCode::NO_CONTENT),
                Err(_) => Ok(axum::http::StatusCode::NOT_FOUND),
            }) as Result<_, WokUnknownError>
        };

        app.add_systems(
            Route(&format!("{}/{{id}}", self.builder.path)),
            delete(system),
        );
    }
}
