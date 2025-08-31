use lump::{
    app::{AppBuilder, ConfigureApp},
    prelude::*,
};
use lump_clap::{ClapPlugin, Main, Route, RouteCfg};

#[derive(clap::Parser)]
struct AppArgs {}

#[derive(clap::Parser)]
struct PersonArgs {
    #[clap(short, long)]
    name: String,
}

#[tokio::main]
pub async fn main() {
    AppBuilder::default()
        .add_plugin(ClapPlugin::parser::<AppArgs>())
        .add_system(Startup, connect_to_db)
        .add_system(Main, do_main)
        .add_system(Route("person"), |cfg| cfg.cfg(add_more_routes).finish())
        .build()
        .run(tokio::runtime::Handle::current(), lump_clap::clap_runtime)
        .await
        .unwrap();
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

fn add_more_routes(cfg: &mut RouteCfg<'_>) {
    cfg.single(for_person);
}
