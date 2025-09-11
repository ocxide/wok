#![allow(non_snake_case)]
use futures::FutureExt;

use crate::{
    async_executor::AsyncExecutor,
    world::{WorldState, gateway::RemoteWorldMut},
};

pub trait RuntimeAddon: Sized {
    /// A set of data to be kept alive until main loop ends
    type Rests: Sized;

    fn create(state: &mut WorldState) -> (Self, Self::Rests);
    fn tick(&mut self) -> impl Future<Output = Option<()>>;
    fn act(&mut self, async_executor: &impl AsyncExecutor, state: &mut RemoteWorldMut<'_>);
}

macro_rules! impl_runtime {
    ( $( $ty:ident : $ty_rests:ident ),* ) => {
        impl<$($ty: RuntimeAddon),*> RuntimeAddon for ($($ty),*) {
            type Rests = ($($ty::Rests),*);

            fn create(state: &mut WorldState) -> (Self, Self::Rests) {
                $( let ($ty,$ty_rests) = $ty::create(state); )*
                (($($ty),*), ($($ty_rests),*))
            }

            async fn tick(&mut self) -> Option<()> {
                let ($($ty),*) = self;

                futures::select! {
                    $($ty = $ty.tick().fuse() => $ty),*
                }
            }

            fn act(&mut self, async_executor: &impl AsyncExecutor, state: &mut RemoteWorldMut<'_>) {
                #[allow(non_snake_case)]
                let ($($ty),*) = self;
                $($ty.act(async_executor, state));*
            }
        }
    };
}

impl_runtime!(R1 : R1_rests, R2 : R2_rests);
impl_runtime!(R1 : R1_rests, R2 : R2_rests, R3 : R3_rests);
impl_runtime!(R1 : R1_rests, R2 : R2_rests, R3 : R3_rests, R4 : R4_rests);

impl RuntimeAddon for () {
    type Rests = ();
    async fn tick(&mut self) -> Option<()> {
        None
    }

    fn act(&mut self, _async_executor: &impl AsyncExecutor, _state: &mut RemoteWorldMut<'_>) {}
    fn create(_state: &mut WorldState) -> (Self, Self::Rests) {
        ((), ())
    }
}
