use lump_core::{
    world::World,
    prelude::{In, IntoSystem, System},
};

async fn first(input8: In<u8>) -> u32 {
    input8.0 as u32
}

async fn second(input32: In<u32>) -> u64 {
    input32.0 as u64
}

async fn third(_: In<u64>) {}

fn main() {
    let lump = World::default();
    let sys = first.pipe(second).pipe(third).into_system();

    let fut = sys.run(&lump.state, 2);
    std::mem::drop(fut);
}
