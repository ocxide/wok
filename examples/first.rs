#![allow(unused)]
use lump::prelude::*;

#[derive(Clone)]
struct State {}
impl Resource for State {}

#[derive(Clone)]
struct State2 {}
impl Resource for State2 {}

async fn single() {}
async fn single_in(_input: In<u32>) {}
async fn single_in_ref(_input: InRef<'_, u32>) {}
async fn res(_state: Res<'_, State>) {}
async fn res_2(_state: Res<'_, State>, _state2: Res<'_, State2>) {}
async fn res_3(_state: Res<'_, State>, _state2: Res<'_, State2>, _input: Res<'_, State2>) {}
async fn in_res(_input: In<u32>, _state: Res<'_, State>) {}
async fn in_res_2(_input: In<u32>, _state: Res<'_, State>, _state2: Res<'_, State2>) {}
async fn in_res_3(
    _input: In<u32>,
    _state: Res<'_, State>,
    _state2: Res<'_, State2>,
    _input2: Res<'_, State2>,
) {
}
async fn long_res(
    _state: Res<'_, State>,
    _state2: Res<'_, State2>,
    _input: Res<'_, State2>,
    _input2: Res<'_, State2>,
    _state3: Res<'_, State>,
    _state7: Res<'_, State2>,
    _input4: Res<'_, State2>,
) {
}
async fn long_in(
    _input0: In<State2>,
    _state2: Res<'_, State2>,
    _input: Res<'_, State2>,
    _input2: Res<'_, State2>,
    _state3: Res<'_, State>,
    _state7: Res<'_, State2>,
    _input4: Res<'_, State2>,
) {
}

fn blocking_reserver<In: SystemInput + 'static>(_: In::Wrapped<'_>) {}

fn main() {
    // let _ = single.into_system();
    // let _ = single_in.into_system();
    let _ = res.into_system();
    let _ = res_2.into_system();
    let _ = res_3.into_system();
    let _ = in_res.into_system();
    let _ = in_res_2.into_system();
    let _ = in_res_3.into_system();
    let _ = long_res.into_system();
    let _ = long_in.into_system();
    let _ = single_in_ref.into_system();
    let _ = blocking_reserver::<In<u32>>.into_system();
}
