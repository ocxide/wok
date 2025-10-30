use wok::{
    plugin::Plugin,
    prelude::{BorrowMutParam, Commands, Param, ParamRef, Startup, WokUnknownError},
};

pub use origins::*;
pub use wok_assets_derive::AssetsCollection;

#[derive(Param)]
pub struct AssetsCollectionInit<'r, A: AssetsCollection> {
    commands: Commands<'r>,
    _resources_mut: ParamRef<'r, A::Assets>,
}

impl<'r, A: for<'a> AssetsCollection<Assets: Param<AsRef<'a> = A::Assets>>>
    AssetsCollectionInit<'r, A>
{
    pub fn load(&mut self, origin: impl AssetOrigin<A>) -> Result<(), WokUnknownError> {
        origin
            .load()
            .map(|collection| collection.insert_all(&mut self.commands))
    }

    pub fn insert_all(&mut self, collection: A) {
        collection.insert_all(&mut self.commands);
    }
}

pub trait AssetOrigin<T>: Send + Sync + Clone + std::fmt::Display + std::fmt::Debug {
    type ParamMarker: wok::prelude::Param;

    fn load(self) -> Result<T, WokUnknownError>;
    fn plugin() -> impl wok::plugin::Plugin {}
}

pub mod origins {
    use std::path::Path;

    use wok::prelude::WokUnknownError;

    use super::AssetOrigin;

    pub use env::Env;

    #[derive(Clone, Debug)]
    pub struct TomlFile<P: AsRef<Path> + Clone + 'static + Send + Sync>(pub P);

    impl<P: AsRef<Path> + Clone + 'static + Send + Sync> std::fmt::Display for TomlFile<P> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0.as_ref().display())
        }
    }

    impl<T, P> AssetOrigin<T> for TomlFile<P>
    where
        T: serde::de::DeserializeOwned,
        P: AsRef<Path> + Send + Sync + Clone + std::fmt::Debug,
    {
        type ParamMarker = ();
        fn load(self) -> Result<T, WokUnknownError> {
            let buf = std::fs::read_to_string(self.0.as_ref())?;

            toml::from_str(&buf).map_err(Into::into)
        }
    }

    mod env {
        use wok::{
            plugin::Plugin,
            prelude::{ResMutMarker, WokUnknownError},
        };

        use crate::AssetOrigin;

        #[derive(wok::prelude::Resource)]
        pub struct EnvLoaded;

        pub struct EnvPlugin;
        impl Plugin for EnvPlugin {
            fn setup(self, app: &mut wok::prelude::App) {
                use wok::prelude::{ConfigureWorld, Startup};

                app.add_systems(Startup, |_: ResMutMarker<EnvLoaded>| {
                    dotenvy::Finder::new().find();
                    if let Err(why) = dotenvy::dotenv() {
                        tracing::warn!(?why, "Failed to load .env");
                    }
                });
            }
        }

        struct EnvReader<R: std::io::Read>(R);

        impl<R: std::io::Read> std::io::Read for EnvReader<R> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let out = self.0.read(buf)?;

                if let Ok(red) = str::from_utf8(buf) {

                }

                Ok(out)
            }
        }

        fn load_env() {
            dotenvy::Iter::new(reader).load
        }

        #[derive(Clone, Debug)]
        pub struct Env;

        impl std::fmt::Display for Env {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "ENV")
            }
        }

        impl<T> AssetOrigin<T> for Env
        where
            T: serde::de::DeserializeOwned,
        {
            type ParamMarker = ResMutMarker<EnvLoaded>;
            fn load(self) -> Result<T, WokUnknownError> {
                serdenv_toml::builder_default()
                    .lowercased()
                    .deserialize::<T>()
                    .map_err(Into::into)
            }

            fn plugin() -> impl wok::plugin::Plugin {
                EnvPlugin
            }
        }
    }
}

pub trait AssetsCollection: Sized + 'static {
    type Assets: BorrowMutParam;
    fn insert_all(self, commands: &mut Commands);
}

pub struct AssetsReadPlugin<O, A> {
    origin: O,
    _marker: std::marker::PhantomData<fn(A)>,
}

impl<O: AssetOrigin<A> + 'static, A: for<'a> AssetsCollection<Assets: Param<AsRef<'a> = A::Assets>>>
    Plugin for AssetsReadPlugin<O, A>
{
    fn setup(self, app: &mut wok::prelude::App) {
        use wok::prelude::{ConfigureApp, ConfigureWorld};
        let origin = self.origin;

        app.add_plugin(O::plugin());

        app.add_systems(
            Startup,
            move |mut init: AssetsCollectionInit<'_, A>,
                  _: wok::prelude::ParamRef<'_, O::ParamMarker>| {
                init.load(origin.clone())
            },
        );
    }
}

pub struct AssetsPlugin<O, A> {
    origin: O,
    _marker: std::marker::PhantomData<fn(A)>,
}

impl<O, A> Plugin for AssetsPlugin<O, A>
where
    O: AssetOrigin<A::In> + 'static,
    A: for<'a> AssetsCollection<Assets: Param<AsRef<'a> = A::Assets>> + valigate::Valid,
{
    fn setup(self, app: &mut wok::prelude::App) {
        use wok::prelude::{ConfigureApp, ConfigureWorld};

        app.add_plugin(O::plugin());

        #[derive(thiserror::Error, Debug)]
        #[error("`{0}`: {1}")]
        struct Error<O>(O, valigate::ErrorDisplay);

        let origin = self.origin;
        app.add_systems(
            Startup,
            move |mut init: AssetsCollectionInit<'_, A>,
                  _: wok::prelude::ParamRef<'_, O::ParamMarker>| {
                let input = origin.clone().load()?;
                let a = A::parse(input)
                    .map_err(|err| Error(origin.clone(), valigate::ErrorDisplay(err)))?;

                init.insert_all(a);
                Ok(())
            },
        );
    }
}

pub struct AssetsOrigin<O>(pub O);

impl<O> AssetsOrigin<O> {
    pub fn load<A: AssetsCollection + valigate::Valid>(self) -> AssetsPlugin<O, A>
    where
        O: AssetOrigin<A::In>,
    {
        AssetsPlugin {
            origin: self.0,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn read<A: AssetsCollection>(self) -> AssetsReadPlugin<O, A>
    where
        O: AssetOrigin<A>,
    {
        AssetsReadPlugin {
            origin: self.0,
            _marker: std::marker::PhantomData,
        }
    }
}
