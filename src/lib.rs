pub mod prelude {
    pub use crate::app::AppBuilder;
    pub use lump_core::error::LumpUnknownError;
    pub use lump_core::prelude::*;
}

mod startup;

pub mod schedules {
    use lump_core::{
        prelude::{DynSystem, In},
        schedule::{ScheduleConfigure, ScheduleLabel, Systems},
    };

    #[derive(Copy, Clone)]
    pub struct Events;

    impl ScheduleLabel for Events {}

    pub trait Event: Send + Sync + 'static {}

    impl<E: Event> ScheduleConfigure<In<&E>, ()> for Events {
        fn add(
            world: &mut lump_core::world::World,
            systemid: lump_core::world::SystemId,
            system: DynSystem<In<&E>, ()>,
        ) {
            let Some(systems) = world.center.resources.get_mut::<Systems<In<&E>, ()>>() else {
                panic!("events `{}` is not registered", std::any::type_name::<E>());
            };

            systems.add(systemid, system);
        }
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
        configs_load: &'c mut ConfigsServer<'c>,
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

    impl<'s> ConfigsServer<'s> {
        pub fn start(&'s mut self) -> ConfigLoads<'s> {
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

pub mod app;

mod async_rt {
    pub mod tokio {
        use futures::FutureExt;
        use tokio::{runtime::Handle, task::JoinHandle};

        use crate::app::AsyncRuntime;

        pub struct TokioJoinHandle<T>(pub JoinHandle<T>);

        impl<T> Future for TokioJoinHandle<T> {
            type Output = T;
            fn poll(
                mut self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Self::Output> {
                self.0
                    .poll_unpin(cx)
                    .map(|poll| poll.expect("Tokio join handle failed"))
            }
        }

        impl AsyncRuntime for Handle {
            type JoinHandle<T: Send + 'static> = TokioJoinHandle<T>;

            fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<Fut::Output>
            where
                Fut: std::future::Future<Output: Send> + Send + 'static,
            {
                let handle = self.spawn(fut);
                TokioJoinHandle(handle)
            }
        }
    }
}
