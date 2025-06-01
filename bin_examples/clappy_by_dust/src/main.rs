use std::{fmt::Display, str::FromStr};

use dust::{error::DustUnknownError, prelude::Commands};
use dust_clap::{RouterBuilder, RouterCfg};
use dust_db::{
    Record, RecordGenerate,
    db::GenerateId,
    surrealdb::{IdString, SurrealDb, SurrealRecord},
};
use surrealdb::{
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};
use tokio::time::sleep;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct PersonId(idn::IdN<4>);

impl FromStr for PersonId {
    type Err = <idn::IdN<4> as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(PersonId(idn::IdN::from_str(s)?))
    }
}

impl Record for PersonId {
    const TABLE: &'static str = "person";
}

impl RecordGenerate for PersonId {
    fn generate() -> Self {
        PersonId(idn::IdN::new())
    }
}

impl Display for PersonId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl SurrealRecord for PersonId {
    type IdKind = IdString;
}

#[derive(Debug, clap::Args, serde::Serialize, serde::Deserialize)]
pub struct Person {
    #[clap(short, long)]
    name: String,
    #[clap(short, long)]
    age: u32,
}

impl Display for Person {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.name, self.age)
    }
}

async fn connect_db(commands: Commands<'_>) -> Result<(), DustUnknownError> {
    sleep(std::time::Duration::from_secs(1)).await;

    let db = surrealdb::Surreal::<Client>::init();
    db.connect::<Ws>("localhost:8080").await?;
    db.signin(Root {
        username: "root",
        password: "root",
    })
    .await?;
    db.use_ns("test").use_db("test").await?;

    commands.insert_resource(SurrealDb::new(db));
    Ok(())
}

#[tokio::main]
async fn main() {
    let router = RouterBuilder::new(
        RouterCfg::new()
            .use_db::<SurrealDb<Client>>()
            .use_id_strat::<GenerateId>(),
    )
    .by_record::<PersonId>(|r| r.create_by::<Person>().list_by::<Person>().delete().build())
    .build();

    dust_clap::App::default()
        .add_startup_system(connect_db)
        .run(router)
        .await;
}
