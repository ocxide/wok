use std::convert::Infallible;

use lump_core::{prelude::*, world::World};

async fn first(input8: In<u8>) -> u32 {
    input8.0 as u32
}

async fn second(input32: In<u32>) -> u64 {
    input32.0 as u64
}

async fn third(_: In<u64>) {}

fn zero() -> Result<u8, Infallible> {
    Ok(0)
}
async fn a(a: In<u8>) {}

fn main() {
    let lump = World::default();
    let a = zero.try_then(async |a: In<u8>| a.0).into_system();
}
