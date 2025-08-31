use lump::{
    app::{AppBuilder, ConfigureMoreWorld},
    prelude::*,
};
use lump_clap::{ClapPlugin, Main, Route};

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
        .add_system(Main, do_main)
        .add_system(Route("person"), |cfg| cfg.single(for_person))
        .build()
        .run(tokio::runtime::Handle::current(), lump_clap::clap_runtime)
        .await
        .unwrap();
}

async fn do_main(args: In<AppArgs>) -> Result<(), LumpUnknownError> {
    println!("Hello world!");
    Ok(())
}

async fn for_person(args: In<PersonArgs>) -> Result<(), LumpUnknownError> {
    println!("Hello {}!", args.name);
    Ok(())
}
