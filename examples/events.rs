use lump::{app::AppBuilder, prelude::*};

struct MyEvent {}
impl Event for MyEvent {}

async fn handler(_event: OnEvents<'_, MyEvent>) {}

fn main() {
    let _app = AppBuilder::<tokio::runtime::Handle>::default().add_system(Events, handler);
}
