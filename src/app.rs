use lump_core::world::{ConfigureWorld, SystemId, World, WorldCenter, WorldState};

use crate::{
    events::{Event, Events},
    startup::Startup,
};

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

pub trait RuntimeConfig: 'static {
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

pub struct SystemTaskLauncher<'c, C: RuntimeConfig> {
    rt: &'c C::AsyncRuntime,
}

impl<C: RuntimeConfig> SystemTaskLauncher<'_, C> {
    pub fn single(&self, fut: impl Future<Output = SystemId> + Send + 'static) {
        let _ = self.rt.spawn(fut);
    }
}

pub trait ConfigureWorldMore<C: RuntimeConfig>: ConfigureWorld {
    fn register_event<E: Event>(mut self) -> Self {
        Events::register::<C, E>(self.world_mut());
        self
    }
}

impl<C: RuntimeConfig, W: ConfigureWorld> ConfigureWorldMore<C> for W {}
