use lump_core::world::{ConfigureWorld, World, WorldState};

use crate::{
    events::{Event, Events},
    foreign::{ParamsClient, ParamsLenderBuilder},
    runtime::{AsyncRuntime, Invokers, Runtime, RuntimeConfig},
    startup::Startup,
};

pub struct AppBuilder<C: RuntimeConfig> {
    world: World,
    pub(crate) invokers: Invokers<C>,
    pub(crate) lender: ParamsLenderBuilder,
}

impl<C: RuntimeConfig> Default for AppBuilder<C> {
    fn default() -> Self {
        let mut world = World::default();

        Startup::init(&mut world.center);

        Self {
            world,
            invokers: Default::default(),
            lender: Default::default(),
        }
    }
}

impl<AR: AsyncRuntime + 'static> AppBuilder<AR> {
    pub fn build_parts(
        self,
        rt: <AR as RuntimeConfig>::AsyncRuntime,
    ) -> (Runtime<AR>, ParamsClient, WorldState) {
        let (state, center) = self.world.into_parts();

        let rt = Runtime::new(
            center,
            self.invokers,
            (self.lender.lender, self.lender.ports),
            rt,
        );
        (rt, self.lender.client, state)
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

impl<AR: AsyncRuntime + 'static> RuntimeConfig for AR {
    type AsyncRuntime = AR;
}

pub trait ConfigureMoreWorld: ConfigureWorld {
    fn register_event<E: Event>(self) -> Self;
    fn add_plugin(self, plugin: impl crate::plugin::Plugin) -> Self;
}

impl<C: RuntimeConfig> ConfigureMoreWorld for AppBuilder<C> {
    fn register_event<E: Event>(mut self) -> Self {
        Events::register::<C, E>(&mut self);
        self
    }

    fn add_plugin(mut self, plugin: impl crate::plugin::Plugin) -> Self {
        plugin.setup(&mut self);
        self
    }
}

impl<C: RuntimeConfig> ConfigureWorld for &mut AppBuilder<C> {
    fn world(&self) -> &World {
        AppBuilder::world(self)
    }

    fn world_mut(&mut self) -> &mut World {
        AppBuilder::world_mut(self)
    }
}

impl<C: RuntimeConfig> ConfigureMoreWorld for &mut AppBuilder<C> {
    fn register_event<E: Event>(self) -> Self {
        Events::register::<C, E>(self);
        self
    }

    fn add_plugin(self, plugin: impl crate::plugin::Plugin) -> Self {
        plugin.setup(&mut *self);
        self
    }
}
