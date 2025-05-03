use std::sync::Arc;

use dust::prelude::*;

#[derive(Clone)]
struct State {}

impl Resource for State {}

async fn my_system(state: Res<'_, State>) {}

fn main() {
    let mut dust = Dust::default();
    dust.resources.insert(State {});

    let a = my_system.into_system();
}
