use std::fmt::Display;

use dust::{error::DustUnknownError, prelude::Commands};
use dust_clap::RouterBuilder;
use dust_db::{Record, surrealdb::SurrealDb};
use surrealdb::{
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};
use tokio::time::sleep;

#[derive(Debug, Clone, Copy)]
pub struct PersonId;

impl Record for PersonId {
    const TABLE: &'static str = "person";
    fn generate() -> Self {
        PersonId
    }
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

async fn sleeeping() -> Result<(), DustUnknownError> {
    println!("sleeping");

    Ok(())
}

#[tokio::main]
async fn main() {
    let router = RouterBuilder::<SurrealDb<Client>>::default()
        .by_record::<PersonId>(|r| r.create_by::<Person>().list_by::<Person>())
        .build();

    dust_clap::App::default()
        .add_startup_system(connect_db)
        .add_startup_system(sleeeping)
        .run(router)
        .await;
}
