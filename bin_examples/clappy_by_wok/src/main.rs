use wok::{
    app::{App, ConfigureApp},
    prelude::*,
};
use wok_clap::{ClapPlugin, Main, Route, SubRoutes};

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
        .add_systems(Startup, connect_to_db)
        .add_systems(Main, do_main)
        .add_systems(
            Route("person"),
            SubRoutes::default().add(Route("a"), for_person),
        );
}

#[derive(Resource)]
struct A;

async fn connect_to_db(a: ResTake<A>) {
    // ...
}

async fn do_main(_args: In<AppArgs>) -> Result<(), WokUnknownError> {
    println!("Hello world!");
    Ok(())
}

async fn for_person(args: In<PersonArgs>) -> Result<(), WokUnknownError> {
    println!("Hello {}!", args.name);
    Ok(())
}
