use lump_core::world::{ConfigureWorld, World, WorldCenter, WorldState};

use crate::startup::Startup;

pub struct AppBuilder {
    world: World,
}

impl Default for AppBuilder {
    fn default() -> Self {
        let mut world = World::default();

        Startup::init(&mut world.center);

        Self { world }
    }
}

impl AppBuilder {
    pub fn build_parts<C: RuntimeConfig>(self, rt: C::AsyncRuntime) -> (Runtime<C>, WorldState) {
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

impl<C: RuntimeConfig> Runtime<C> {
    pub async fn invoke_startup(&mut self, state: &mut WorldState) {
        let invoker = Startup::create_invoker::<C>(&mut self.world, state, &self.rt);
        invoker.invoke().await
    }
}
