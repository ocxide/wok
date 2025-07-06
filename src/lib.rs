pub mod prelude {
    pub use crate::app::AppBuilder;
    pub use lump_core::error::LumpUnknownError;
    pub use lump_core::prelude::*;

    pub use crate::events::{Event, Events, OnEvents};
}

mod events;
mod startup;

pub(crate) mod runtime;

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

