use std::sync::Arc;

use dust::prelude::*;

#[derive(Clone)]
struct State {}

#[derive(Clone)]
struct State2 {}

impl Resource for State {}
impl Resource for State2 {}

async fn my_system(i: In<u32>, state: Res<'_, State>) {}

fn main() {
    let mut dust = Dust::default();
    dust.resources.insert(State {});

    b(my_system);
}

fn b<M, IS>(system: IS)
where
    IS: IntoSystem<u32, M>,
{
}
