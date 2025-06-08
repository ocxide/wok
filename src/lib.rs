pub mod prelude {
    pub use lump_core::error::LumpUnknownError;
    pub use lump_core::prelude::*;
    pub use crate::app::AppBuilder;
}

pub mod schedules {
    use lump_core::{error::LumpUnknownError, schedule::ScheduleLabel};

    #[derive(Copy, Clone)]
    pub struct Startup;
    impl ScheduleLabel for Startup {
        type SystenIn = ();
        type SystemOut = Result<(), LumpUnknownError>;
    }
}

pub mod config {
    use std::ops::Deref;

    use lump_core::{
        error::LumpUnknownError,
        prelude::{Commands, Param, Res, Resource},
    };

    pub trait Config: Send + Sync + 'static {}

    #[derive(Param)]
    #[param(usage = lib)]
    pub struct ConfigRead<'p, C: Config> {
        res: Res<'p, ConfigResource<C>>,
    }

    impl<C: Config> Deref for ConfigRead<'_, C> {
        type Target = C;

        fn deref(&self) -> &Self::Target {
            &self.res.deref().0
        }
    }

    pub struct ConfigResource<T>(T);
    impl<T: Config> Resource for ConfigResource<T> {}

    pub trait ConfigLoader<T>: Sized + 'static {
        fn load(self) -> Result<T, LumpUnknownError>;
    }

    #[derive(Param)]
    #[param(usage = lib)]
    pub struct ConfigsServer<'w> {
        commands: Commands<'w>,
    }

    pub struct ConfigLoads<'c> {
        configs_load: &'c ConfigsServer<'c>,
    }

    impl ConfigLoads<'_> {
        pub fn load_with<T: Config>(
            self,
            loader: impl ConfigLoader<T>,
        ) -> Result<Self, LumpUnknownError> {
            let value = loader.load()?;
            self.configs_load
                .commands
                .insert_resource(ConfigResource(value));

            Ok(self)
        }
    }

    impl ConfigsServer<'_> {
        pub fn start(&self) -> ConfigLoads<'_> {
            ConfigLoads { configs_load: self }
        }
    }
}

pub mod config_loaders {
    use std::path::PathBuf;

    use lump_core::error::LumpUnknownError;
    use serde::de::DeserializeOwned;

    use crate::config::{Config, ConfigLoader};

    pub struct TomlLoader(pub PathBuf);

    impl<C: Config + DeserializeOwned> ConfigLoader<C> for TomlLoader {
        fn load(self) -> Result<C, LumpUnknownError> {
            let s = std::fs::read_to_string(self.0)?;
            let value = toml::from_str(s.as_str())?;

            Ok(value)
        }
    }
}

pub mod app {
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
        pub fn build_parts<C: RuntimeConfig>(self, config: C) -> (Runtime<C>, WorldState) {
            let rt = config.into_parts();
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
        type JoinHandle<T>: Future<Output = T> + Send + 'static;
        fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<Fut::Output>
        where
            Fut: std::future::Future + Send + 'static;
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

            pub fn invoke_startup(&mut self, state: &WorldState) -> impl Future<Output = ()> {
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
                }
            }
        }
    }
}
