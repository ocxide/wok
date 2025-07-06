use lump_core::world::{ConfigureWorld, World, WorldState};

use crate::{
    events::{Event, Events},
    runtime::{Invokers, Runtime, RuntimeConfig},
    startup::Startup,
};

pub struct AppBuilder<C: RuntimeConfig> {
    world: World,
    pub(crate) invokers: Invokers<C>,
}

impl<C: RuntimeConfig> Default for AppBuilder<C> {
    fn default() -> Self {
        let mut world = World::default();

        Startup::init(&mut world.center);

        Self {
            world,
            invokers: Default::default(),
        }
    }
}

impl<C: RuntimeConfig> AppBuilder<C> {
    pub fn build_parts(self, rt: C::AsyncRuntime) -> (Runtime<C>, WorldState) {
        let (state, center) = self.world.into_parts();

        let rt = Runtime::<C>::new(center, self.invokers, rt);
        (rt, state)
    }
}

impl<C: RuntimeConfig> ConfigureWorld for AppBuilder<C> {
    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}
impl<C: RuntimeConfig> AppBuilder<C> {
    pub fn register_event<E: Event>(mut self) -> Self {
        Events::register::<C, E>(&mut self);
        self
    }
}
