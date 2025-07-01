use lump::{app::AppBuilder, prelude::*};

struct MyEvent {}
impl Event for MyEvent {}

async fn handler(_event: In<&MyEvent>) {}

fn main() {
    let _app = AppBuilder::default().add_system(Events, handler);
}
