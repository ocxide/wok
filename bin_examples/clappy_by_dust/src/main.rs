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

#[tokio::main]
async fn main() {
    let mut dust = dust::Dust::default();

    let db = surrealdb::Surreal::<Client>::init();
    db.connect::<Ws>("localhost:8080").await.unwrap();
    db.signin(Root {
        username: "root",
        password: "root",
    })
    .await
    .unwrap();
    db.use_ns("test").use_db("test").await.unwrap();

    dust.resources.insert(SurrealDb::<Client>::new(db));

    let (command, systems) = dust_clap::ClapRecordSystems::<SurrealDb<Client>, PersonId>::default()
        .create_by::<CreatePerson>()
        .build();

    if let Some((command_name, args)) = command
        .subcommand_required(true)
        .arg_required_else_help(true)
        .get_matches()
        .remove_subcommand()
    {
        let system = systems.get(&command_name).unwrap();
        system.run(&dust, args).await;
    }
}
