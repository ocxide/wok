use dust::prelude::Commands;
use dust_clap::RouterBuilder;
use dust_db::{Record, surrealdb::SurrealDb};
use surrealdb::{
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};

#[derive(Debug, Clone, Copy)]
pub struct PersonId;

impl Record for PersonId {
    const TABLE: &'static str = "person";
    fn generate() -> Self {
        PersonId
    }
}

#[derive(Debug, clap::Args, serde::Serialize)]
pub struct CreatePerson {}

async fn connect_db(commands: Commands<'_>) {
    let db = surrealdb::Surreal::<Client>::init();
    db.connect::<Ws>("localhost:8080").await.unwrap();
    db.signin(Root {
        username: "root",
        password: "root",
    })
    .await
    .unwrap();
    db.use_ns("test").use_db("test").await.unwrap();

    commands.insert_resource(SurrealDb::new(db));
}

#[tokio::main]
async fn main() {
    let router = RouterBuilder::<SurrealDb<Client>>::default()
        .by_record::<PersonId>(|r| r.create_by::<CreatePerson>())
        .build();

    dust_clap::App::default()
        .add_startup_system(connect_db)
        .run(router)
        .await;
}
