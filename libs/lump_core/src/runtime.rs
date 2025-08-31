use crate::{async_executor::AsyncExecutor, system_locking::StateLocker, world::WorldState};

pub trait RuntimeAddon {
    fn create(state: &mut WorldState) -> Self;
    fn tick(&mut self) -> impl Future<Output = Option<()>>;
    fn act(&mut self, async_executor: &impl AsyncExecutor, state: &mut StateLocker<'_>);
}

macro_rules! impl_runtime {
    ( $( $ty:ident ),* ) => {
        impl<$($ty: RuntimeAddon),*> RuntimeAddon for ($($ty),*) {
            fn create(state: &mut WorldState) -> Self {
                ($($ty::create(state)),*)
            }

            async fn tick(&mut self) -> Option<()> {
                #[allow(non_snake_case)]
                let ($($ty),*) = self;
                #[allow(non_snake_case)]
                let ($($ty),*) = futures::join!($($ty.tick()),*);
                $($ty?;)*
                Some(())
            }

            fn act(&mut self, async_executor: &impl AsyncExecutor, state: &mut StateLocker<'_>) {
                #[allow(non_snake_case)]
                let ($($ty),*) = self;
                $($ty.act(async_executor, state));*
            }
        }
    };
}

impl_runtime!(R1, R2);
impl_runtime!(R1, R2, R3);
impl_runtime!(R1, R2, R3, R4);
impl_runtime!(R1, R2, R3, R4, R5);

impl RuntimeAddon for () {
    async fn tick(&mut self) -> Option<()> {
        Some(())
    }
    fn act(&mut self, _async_executor: &impl AsyncExecutor, _state: &mut StateLocker<'_>) {}
    fn create(_state: &mut WorldState) -> Self {}
}
