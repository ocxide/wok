use lump_core::world::{ConfigureWorld, World, WorldCenter, WorldState};

use crate::schedules::Startup;

pub struct AppBuilder {
    world: World,
}

impl Default for AppBuilder {
    fn default() -> Self {
        let mut world = World::default();
        world.init_schedule::<Startup>();

        Self { world }
    }
}

impl AppBuilder {
    pub fn build_parts<C: RuntimeConfig>(
        self,
        rt: C::AsyncRuntime,
    ) -> (Runtime<C>, WorldState) {
        let (state, center) = self.world.into_parts();

        let rt = Runtime::<C> { world: center, rt };
        (rt, state)
    }
}

impl ConfigureWorld for AppBuilder {
    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

pub trait AsyncRuntime {
    type JoinHandle<T: Send + 'static>: Future<Output = T> + Send + 'static;
    fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<Fut::Output>
    where
        Fut: std::future::Future<Output: Send> + Send + 'static;
}

pub trait RuntimeConfig {
    type AsyncRuntime: AsyncRuntime;

    fn into_parts(self) -> Self::AsyncRuntime;
}

pub struct Runtime<C: RuntimeConfig> {
    world: WorldCenter,
    rt: C::AsyncRuntime,
}

mod startup {
    use futures::{StreamExt, stream::FuturesUnordered};
    use lump_core::{
        schedule::LabeledScheduleSystem,
        world::{SystemId, WorldState, WorldSystemRunError},
    };

    use crate::schedules::Startup;

    use super::*;

    impl<C: RuntimeConfig> Runtime<C> {
        fn pending_systems(
            &mut self,
            schedule: &mut LabeledScheduleSystem<Startup>,
            state: &WorldState,
            futures: &mut FuturesUnordered<
                <C::AsyncRuntime as AsyncRuntime>::JoinHandle<SystemId>,
            >,
        ) {
            for _ in schedule.schedule.extract_if(move |id, system| {
                match self.world.try_access(id) {
                    Ok(_) => {}
                    Err(WorldSystemRunError::NotRegistered) => {
                        panic!("System not registered")
                    }
                    Err(WorldSystemRunError::InvalidAccess) => return false,
                };

                let fut = system.run(state, ());
                let fut = self.rt.spawn(async move {
                    let _ = fut.await;
                    id
                });

                futures.push(fut);

                true
            }) {}
        }

        pub fn invoke_startup(&mut self, state: &mut WorldState) -> impl Future<Output = ()> {
            let mut schedule = self
                .world
                .resources
                .try_take::<LabeledScheduleSystem<Startup>>()
                .expect("Failed to take schedule");

            let mut futures = FuturesUnordered::new();
            self.pending_systems(&mut schedule, state, &mut futures);

            async move {
                while let Some(systemid) = futures.next().await {
                    self.world.release_access(systemid);
                    self.pending_systems(&mut schedule, state, &mut futures);
                }

                self.world.tick_commands(state).await;
            }
        }
    }
}
