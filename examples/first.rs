use std::sync::Arc;

use dust::{prelude::*, system_fn::SystemFn};

#[derive(Clone)]
struct State {}

impl Resource for State {}

async fn my_system(state: Res<State>) {}

fn main() {
    let mut dust = Dust::default();
    dust.resources.insert(State {});
}
