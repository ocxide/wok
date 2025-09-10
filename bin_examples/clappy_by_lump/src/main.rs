use lump::{
    app::{App, ConfigureApp},
    prelude::*,
};
use lump_clap::{ClapPlugin, Main, Route, SubRoutes};

#[derive(clap::Parser)]
struct AppArgs {}

#[derive(clap::Parser)]
struct PersonArgs {
    #[clap(short, long)]
    name: String,
}

#[tokio::main]
pub async fn main() {
    App::default()
        .add_plugin(ClapPlugin::parser::<AppArgs>())
        .add_system(Startup, connect_to_db)
        .add_system(Main, do_main)
        .add_system(
            Route("person"),
            SubRoutes::default().add(Route("a"), for_person),
        );
}

async fn connect_to_db() {
    // ...
}

async fn do_main(_args: In<AppArgs>) -> Result<(), LumpUnknownError> {
    println!("Hello world!");
    Ok(())
}

async fn for_person(args: In<PersonArgs>) -> Result<(), LumpUnknownError> {
    println!("Hello {}!", args.name);
    Ok(())
}
