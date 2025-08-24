use lump_core::{
    error::LumpUnknownError,
    prelude::{IntoSystem, System},
    world::{ConfigureWorld, World, WorldState},
};

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

    pub fn build(self, rt: <AR as RuntimeConfig>::AsyncRuntime) -> App<AR> {
        let (rt, client, mut state) = self.build_parts(rt);

        state.resources.insert(client);

        App { rt, state }
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

pub struct App<AR: AsyncRuntime + 'static> {
    pub state: WorldState,
    rt: Runtime<AR>,
}

impl<AR: AsyncRuntime + 'static> App<AR> {
    pub async fn run<S, Marker>(mut self, system: S) -> Result<(), LumpUnknownError>
    where
        S: IntoSystem<Marker>,
        S::System: System<In = (), Out = Result<(), LumpUnknownError>>,
    {
        self.rt.invoke_startup(&mut self.state).await?;

        let client = self.state.get_resource::<ParamsClient>();
        let client = client.read().expect("to get client").clone();

        let main_fut = client.run(system, ());

        let bg_fut = async {
            self.rt.run(&self.state).await;
            Ok(())
        };

        futures::future::try_join(main_fut, bg_fut).await?;

        Ok(())
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
