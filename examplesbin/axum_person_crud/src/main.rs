#![allow(dead_code)] // for demo
use wok::{prelude::*, setup::*};
use wok_axum::AxumPlugin;

// a resource system for demo purposes
#[derive(Resource)]
enum EnvMode {
    Dev,
    Prod,
}

#[tokio::main]
async fn main() -> Result<(), MainError> {
    App::default()
        // a resource system for demo purposes
        .insert_resource(EnvMode::Dev)
        .add_plugin(AxumPlugin)
        .add_plugin(config::ConfigPlugin)
        .add_plugin(persons::PersonsPlugin)
        .add_plugin(colors::ColorsPlugin)
        .add_systems(Startup, validate_dev_localhost)
        .run(RuntimeCfg::default().with_async(TokioRt), wok_axum::serve)
        .await?;

    Ok(())
}

// This is how you would create a startup system that validates the host if we are in dev mode
fn validate_dev_localhost(
    env_mode: Res<'_, EnvMode>,
    host: Res<'_, wok_axum::SocketAddrs>,
) -> Result<(), WokUnknownError> {
    if let (EnvMode::Prod, wok_axum::SocketAddrs::Single(wok_axum::Addr::Unresolved(host))) =
        (env_mode.as_ref(), host.as_ref())
        && host.contains("localhost")
    {
        return Err(WokUnknownError::from_message(
            "dev mode is not allowed on localhost",
        ));
    }

    Ok(())
}

mod config {
    use surrealdb::engine::remote::ws::{Client, Ws};
    use wok::prelude::*;
    use wok_assets::{AssetsOrigin, TomlFile};
    use wok_axum::crud::CRUDCfgBuilder;
    use wok_db::{
        id_strategy::GenerateId,
        surrealdb::{RemoteSurrealDbPlugin, SurrealDb},
    };

    pub type Db = SurrealDb<Client>;

    #[derive(valigate::Valid, wok_assets::AssetsCollection)]
    #[gate(serde = true)]
    pub struct Config {
        db: wok_db::surrealdb::SurrealUseDb,
    }

    #[derive(valigate::Valid, wok_assets::AssetsCollection)]
    #[gate(serde = true)]
    pub struct Env {
        db: wok_db::surrealdb::SurrealCredentials,
        #[gate(skip = true)] // Skip validation on fields that do only care about its contents and
        // apply no validation; SocketAddrs does not impl Valid trait because its raw input
        host: wok_axum::SocketAddrs,
    }

    pub struct ConfigPlugin;

    impl Plugin for ConfigPlugin {
        fn setup(self, app: &mut App) {
            // This will load a config.toml file from the current directory
            app.add_plugin(AssetsOrigin(TomlFile("config.toml")).load::<Config>())
                // This will load a .env file from the current directory
                .add_plugin(AssetsOrigin(wok_assets::Env).load::<Env>())
                .add_plugin(RemoteSurrealDbPlugin::<Ws>::default());
        }
    }

    pub fn db_config_factory() -> CRUDCfgBuilder<Db, GenerateId> {
        CRUDCfgBuilder::default().db::<Db>().id::<GenerateId>()
    }
}

mod persons {
    use valigate::gates::{LessThan, MaxLen, MinLen};
    use wok::prelude::*;
    use wok_axum::{Route, crud::CrudConfig, extract::JsonG, post, response::Created};
    use wok_db::{
        RecordGenerate,
        db::{DbQuery, RecordDb},
        surrealdb::{AsSurrealBind, FromSurrealBind},
    };

    use crate::{
        colors::ColorId,
        config::{Db, db_config_factory},
    };

    pub struct PersonsPlugin;
    impl Plugin for PersonsPlugin {
        fn setup(self, app: &mut App) {
            let factory = db_config_factory().for_record::<PersonId>();

            app.add_plugin(factory.list_all::<Person>())
                .add_plugin(factory.delete_one())
                .add_plugin(factory.get_one::<Person>())
                // .add_plugin(factory.create_one::<Person>()) // We are implementing this one ourselves for
                // demo purposes
                .add_systems(Route("/persons"), post(create_one));
        }
    }

    #[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct PersonId(idn::IdN<8>);

    impl RecordGenerate for PersonId {
        fn generate() -> Self {
            PersonId(idn::IdN::default())
        }
    }

    impl wok_db::Record for PersonId {
        const TABLE: &'static str = "person";
    }

    impl wok_db::surrealdb::SurrealRecord for PersonId {
        type Flavor = wok_db::surrealdb::StringFlavor;
    }

    #[derive(valigate::Valid, AsSurrealBind, FromSurrealBind, serde::Serialize)]
    #[gate(serde = true)]
    struct Person {
        name: PersonName,
        age: PersonAge,
        #[gate(skip = true)] // RecordIds do not impl Valid trait, so we skip validation
        favorite_color: ColorId,
    }

    #[derive(valigate::Valid, AsSurrealBind, FromSurrealBind, serde::Serialize)]
    #[gate(gate = (
        StartsWithMayus,
        MinLen(4),
        MaxLen(200)
    ))]
    pub struct PersonName(String);

    #[derive(valigate::Valid, AsSurrealBind, FromSurrealBind, serde::Serialize)]
    #[gate(gate = LessThan(100))]
    pub struct PersonAge(usize);

    struct StartsWithMayus;

    #[derive(thiserror::Error, Debug)]
    #[error("name must start with mayus")]
    struct StartsWithMayusErr;

    impl valigate::Gate<String> for StartsWithMayus {
        type Out = String;
        type Err = StartsWithMayusErr;

        fn parse(self, input: String) -> valigate::GateResult<Self::Out, Self::Err> {
            if input.starts_with(|c: char| c.is_ascii_uppercase()) {
                valigate::GateResult::Ok(input)
            } else {
                valigate::GateResult::ErrPass(input, StartsWithMayusErr)
            }
        }
    }

    // JsonG validates the request body using valigate crate!
    async fn create_one(
        In(JsonG(data)): In<JsonG<Person>>,
        db: Res<'_, Db>,
    ) -> Result<Created<axum::Json<PersonId>>, WokUnknownError> {
        let id = db.record::<PersonId>().create(data).execute().await?;
        Ok(Created::Created(axum::Json(id)))
    }
}

mod colors {
    use axum::{Json, extract::Query};
    use valigate::gates::MinLen;
    use wok::prelude::*;
    use wok_axum::{Route, crud::CrudConfig, get};
    use wok_db::{
        RecordGenerate,
        db::{DbQuery, RecordDb},
        surrealdb::{AsSurrealBind, FromSurrealBind},
    };

    use crate::config::{Db, db_config_factory};

    pub struct ColorsPlugin;
    impl Plugin for ColorsPlugin {
        fn setup(self, app: &mut App) {
            let factory = db_config_factory().for_record::<ColorId>();

            app.add_systems(Route("/colors"), get(list))
                .add_plugin(factory.delete_one())
                .add_plugin(factory.get_one::<Color>())
                .add_plugin(factory.create_one::<Color>());
        }
    }

    #[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct ColorId(idn::IdN<8>);

    impl wok_db::Record for ColorId {
        const TABLE: &'static str = "color";
    }

    impl wok_db::surrealdb::SurrealRecord for ColorId {
        type Flavor = wok_db::surrealdb::StringFlavor;
    }

    impl RecordGenerate for ColorId {
        fn generate() -> Self {
            ColorId(idn::IdN::default())
        }
    }

    #[derive(valigate::Valid, AsSurrealBind, FromSurrealBind, serde::Serialize)]
    #[gate(serde = true)]
    struct Color {
        name: ColorName,
        #[gate(skip = true)]
        value: String,
    }

    #[derive(valigate::Valid, AsSurrealBind, FromSurrealBind, serde::Serialize)]
    #[gate(gate = MinLen(1))]
    pub struct ColorName(String);

    #[derive(serde::Deserialize)]
    struct QueryFilter {
        name: Option<String>,
    }

    // Can manually implement listing if needed
    async fn list(
        In(Query(query)): In<Query<QueryFilter>>,
        db: Res<'_, Db>,
    ) -> Result<Json<Vec<Color>>, WokUnknownError> {
        let colors = if let Some(name) = query.name {
            // Can always use underline surrealdb API to create complex queries
            let mut response =
                db.0.query("SELECT * FROM color WHERE name CONTAINS $name")
                    .bind(("name", name))
                    .await?;

            let colors: Vec<<Color as FromSurrealBind>::Bind> = response.take(0)?;
            colors.into_iter().map(Color::from_bind).collect()
        } else {
            db.record::<ColorId>().list::<Color>().execute().await?
        };

        Ok(Json(colors))
    }
}
