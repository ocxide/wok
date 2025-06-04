pub mod prelude {
    pub use lump_core::error::LumpUnknownError;
    pub use lump_core::prelude::*;
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

    pub struct ConfigRead<'p, C: Config> {
        res: Res<'p, ConfigResource<C>>,
    }

    impl<'p, C: Config> Param for ConfigRead<'p, C> {
        type Owned = <Res<'p, ConfigResource<C>> as Param>::Owned;
        type AsRef<'r> = ConfigRead<'r, C>;

        fn get(world: &lump_core::prelude::World) -> Self::Owned {
            <Res<'_, ConfigResource<C>> as Param>::get(world)
        }

        fn as_ref(owned: &Self::Owned) -> Self::AsRef<'_> {
            ConfigRead {
                res: Res::as_ref(owned),
            }
        }
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

    pub struct ConfigsServer<'p> {
        commands: Commands<'p>,
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

    impl<'p> Param for ConfigsServer<'p> {
        type Owned = <Commands<'p> as Param>::Owned;
        type AsRef<'r> = ConfigsServer<'r>;

        fn get(world: &lump_core::prelude::World) -> Self::Owned {
            <Commands<'_> as Param>::get(world)
        }

        fn as_ref(owned: &Self::Owned) -> Self::AsRef<'_> {
            ConfigsServer {
                commands: <Commands<'_> as Param>::as_ref(owned),
            }
        }
    }
}

pub mod config_loaders {
    use std::path::PathBuf;

    use lump_core::error::LumpUnknownError;
    use serde::de::DeserializeOwned;

    use crate::config::{Config, ConfigLoader};

    pub struct TomlLoader(PathBuf);

    impl<C: Config + DeserializeOwned> ConfigLoader<C> for TomlLoader {
        fn load(self) -> Result<C, LumpUnknownError> {
            let s = std::fs::read_to_string(self.0)?;
            let value = toml::from_str(s.as_str())?;

            Ok(value)
        }
    }
}
