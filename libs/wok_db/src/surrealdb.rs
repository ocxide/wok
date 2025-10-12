use record_serde::ThingOwned;
use surrealdb::{Connection, Surreal};

pub use as_surreal_bind::{AsSurrealBind, SurrealSerialize};
pub use from_surreal_bind::FromSurrealBind;
pub use record_serde::{IdFlavor, StringFlavor, SurrealRecord};

use crate::{
    RecordEntry, RecordGenerate,
    id_strategy::{GenerateId, IdStrategy},
};

mod as_surreal_bind;
mod crud;
mod from_surreal_bind;
mod record_serde;

#[derive(wok::prelude::Resource)]
pub struct SurrealDb<C: Connection>(pub Surreal<C>);

impl<C: Connection> SurrealDb<C> {
    #[inline]
    pub const fn new(db: Surreal<C>) -> Self {
        SurrealDb(db)
    }
}

pub struct KeyValue<R: SurrealRecord, B> {
    pub id: R,
    pub data: B,
}

#[derive(serde::Deserialize)]
pub struct FromRecordEntrySurreal<R, D> {
    pub id: R,
    #[serde(flatten)]
    pub data: D,
}

#[derive(serde::Serialize)]
pub struct SurrealKeyValueRef<'b, R: SurrealRecord, B: AsSurrealBind> {
    pub id: &'b R,
    #[serde(flatten)]
    pub data: B::Bind<'b>,
}

impl<R: SurrealRecord, B: AsSurrealBind> AsSurrealBind for KeyValue<R, B> {
    type Bind<'b> = SurrealKeyValueRef<'b, R, B>;
    fn as_bind(&self) -> Self::Bind<'_> {
        SurrealKeyValueRef {
            id: &self.id,
            data: self.data.as_bind(),
        }
    }
}

impl<R: SurrealRecord + RecordGenerate> IdStrategy<R> for GenerateId {
    type Wrap<D> = KeyValue<R, D>;
    fn wrap<D>(body: D) -> Self::Wrap<D> {
        KeyValue {
            id: R::generate(),
            data: body,
        }
    }
}

impl<R: SurrealRecord, D: AsSurrealBind> AsSurrealBind for RecordEntry<R, D> {
    type Bind<'b> = SurrealKeyValueRef<'b, R, D>;
    fn as_bind(&self) -> Self::Bind<'_> {
        SurrealKeyValueRef {
            id: &self.id,
            data: self.data.as_bind(),
        }
    }
}

impl<R: SurrealRecord, D: FromSurrealBind> FromSurrealBind for RecordEntry<R, D> {
    type Bind = FromRecordEntrySurreal<ThingOwned<R>, D::Bind>;

    fn from_bind(bind: Self::Bind) -> Self {
        Self {
            id: R::from_owned(bind.id),
            data: D::from_bind(bind.data),
        }
    }
}

#[derive(serde::Deserialize, wok::prelude::Resource, Debug)]
pub struct SurrealCredentials {
    pub host: String,
    #[serde(flatten)]
    pub signin: Option<SurrealSignIn>,
    #[serde(flatten)]
    pub using_db: Option<SurrealUsingDb>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SurrealSignIn {
    pub username: String,
    pub password: String,
}

#[derive(wok::prelude::Resource)]
pub struct SurrealConfig(pub surrealdb::opt::Config);

#[derive(Debug, serde::Deserialize)]
pub struct SurrealUsingDb {
    pub database: String,
    pub namespace: String,
}

pub struct RemoteSurrealDbPlugin<P: 'static>(std::marker::PhantomData<P>);

impl<P: 'static> Default for RemoteSurrealDbPlugin<P> {
    fn default() -> Self {
        RemoteSurrealDbPlugin(std::marker::PhantomData)
    }
}

impl<P: 'static> wok::prelude::Plugin for RemoteSurrealDbPlugin<P>
where
    (String, surrealdb::opt::Config):
        surrealdb::opt::IntoEndpoint<P, Client = surrealdb::engine::remote::ws::Client>,
{
    fn setup(self, app: &mut wok::prelude::App) {
        use wok::prelude::{ConfigureWorld, ResInit, ResTake};
        app.add_systems(
            wok::prelude::Startup,
            async |creds: ResTake<SurrealCredentials>,
                   config: Option<ResTake<SurrealConfig>>,
                   mut surreal: ResInit<'_, SurrealDb<surrealdb::engine::remote::ws::Client>>|
                   -> Result<(), wok::prelude::WokUnknownError> {
                let creds = creds.into_inner();
                let config = config.map(|c| c.into_inner().0).unwrap_or_default();

                tracing::info!("Connected to remote SurrealDB at '{}'", &creds.host);
                let db = surrealdb::Surreal::<surrealdb::engine::remote::ws::Client>::new::<P>((
                    creds.host, config,
                ))
                .await?;

                if let Some(signin) = creds.signin {
                    db.signin(surrealdb::opt::auth::Root {
                        username: &signin.username,
                        password: &signin.password,
                    })
                    .await?;

                    tracing::info!("Signed in as '{}'", &signin.username);
                } else {
                    tracing::warn!("Credentials missing for remote SurrealDB");
                }

                if let Some(using_db) = creds.using_db {
                    tracing::info!(
                        "Using namespace '{}' and database '{}'",
                        &using_db.namespace,
                        &using_db.database
                    );

                    db.use_ns(using_db.namespace)
                        .use_db(using_db.database)
                        .await?;
                } else {
                    tracing::warn!("Using no database for remote SurrealDB");
                }

                surreal.init(SurrealDb::new(db));

                Ok(())
            },
        );
    }
}
