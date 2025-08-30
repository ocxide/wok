use lump::{
    app::{AppBuilder, ConfigureMoreWorld},
    prelude::*,
};
use lump_clap::{ClapPlugin, Main};

#[derive(clap::Parser)]
struct AppArgs {}

#[tokio::main]
pub async fn main() {
    let a = AppBuilder::default()
        .add_plugin(ClapPlugin::parser::<AppArgs>())
        .add_system(Main, main_route)
        .build()
        .run(tokio::runtime::Handle::current(), lump_clap::clap_runtime)
        .await;
}

async fn main_route(args: In<AppArgs>) -> Result<(), LumpUnknownError> {
    println!("hi!");
    Ok(())
}
