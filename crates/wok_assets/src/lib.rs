use wok::{
    plugin::Plugin,
    prelude::{BorrowMutParam, Commands, Param, Startup, WokUnknownError},
};

pub use origins::*;
pub use wok_assets_derive::AssetsCollection;

#[derive(Param)]
pub struct AssetsCollectionInit<
    'r,
    A: for<'a> AssetsCollection<Assets: Param<AsRef<'a> = A::Assets>>,
> {
    commands: Commands<'r>,
    _resources_mut: A::Assets,
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

pub trait AssetOrigin<T>: Send + Sync + Clone {
    type ParamMarker: wok::prelude::Param;

    fn load(self) -> Result<T, WokUnknownError>;
    fn plugin() -> impl wok::plugin::Plugin {}
}

pub mod origins {
    use std::path::Path;

    use wok::prelude::WokUnknownError;

    use super::AssetOrigin;

    pub use env::Env;

    pub struct TomlFile<P: AsRef<Path> + Clone + 'static + Send + Sync>(pub P);

    impl<P: AsRef<Path> + 'static + Clone + Send + Sync> Clone for TomlFile<P> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T, P> AssetOrigin<T> for TomlFile<P>
    where
        T: serde::de::DeserializeOwned,
        P: AsRef<Path> + Send + Sync + Clone,
    {
        type ParamMarker = ();
        fn load(self) -> Result<T, WokUnknownError> {
            let buf = std::fs::read_to_string(self.0.as_ref())?;

            toml::from_str(&buf).map_err(Into::into)
        }
    }

    mod env {
        use wok::{plugin::Plugin, prelude::{ResMutMarker, WokUnknownError}};

        use crate::AssetOrigin;

        #[derive(wok::prelude::Resource)]
        pub struct EnvLoaded;

        pub struct EnvPlugin;
        impl Plugin for EnvPlugin {
            fn setup(self, app: &mut wok::prelude::App) {
                use wok::prelude::{ConfigureWorld, Startup};

                app.add_systems(Startup, |_: ResMutMarker<EnvLoaded>| {
                    dotenv::dotenv().map_err(WokUnknownError::from)?;
                    Ok(())
                });
            }
        }

        #[derive(Clone)]
        pub struct Env;
        impl<T> AssetOrigin<T> for Env
        where
            T: serde::de::DeserializeOwned,
        {
            type ParamMarker = ResMutMarker<EnvLoaded>;
            fn load(self) -> Result<T, WokUnknownError> {
                envy::from_env().map_err(Into::into)
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

pub struct AssetsReadPlugin<O, A = ()> {
    origin: O,
    _marker: std::marker::PhantomData<fn(A)>,
}

impl<O> AssetsReadPlugin<O, ()> {
    pub const fn origin(origin: O) -> Self {
        AssetsReadPlugin {
            origin,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn assets<A: AssetsCollection>(self) -> AssetsReadPlugin<O, A>
    where
        O: AssetOrigin<A>,
    {
        AssetsReadPlugin {
            origin: self.origin,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<O: AssetOrigin<A> + 'static, A: for<'a> AssetsCollection<Assets: Param<AsRef<'a> = A::Assets>>>
    Plugin for AssetsReadPlugin<O, A>
{
    fn setup(self, app: &mut wok::prelude::App) {
        use wok::prelude::ConfigureWorld;
        let origin = self.origin;
        app.add_systems(Startup, move |mut init: AssetsCollectionInit<'_, A>| {
            init.load(origin.clone())
        });
    }
}

pub struct AssetsPlugin<O, A = ()> {
    origin: O,
    _marker: std::marker::PhantomData<fn(A)>,
}

impl<O> AssetsPlugin<O, ()> {
    pub const fn origin(origin: O) -> Self {
        AssetsPlugin {
            origin,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn assets<A: AssetsCollection + valigate::Valid>(self) -> AssetsPlugin<O, A>
    where
        O: AssetOrigin<A::In>,
    {
        AssetsPlugin {
            origin: self.origin,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<
    O: AssetOrigin<A::In> + 'static,
    A: for<'a> AssetsCollection<Assets: Param<AsRef<'a> = A::Assets>> + valigate::Valid,
> Plugin for AssetsPlugin<O, A>
{
    fn setup(self, app: &mut wok::prelude::App) {
        use wok::prelude::{ConfigureApp, ConfigureWorld};

        app.add_plugin(O::plugin());

        let origin = self.origin;
        app.add_systems(
            Startup,
            move |mut init: AssetsCollectionInit<'_, A>,
                  _: wok::prelude::ParamRef<'_, O::ParamMarker>| {
                let input = origin.clone().load()?;
                let a = A::parse(input).map_err(valigate::ErrorDisplay)?;

                init.insert_all(a);
                Ok(())
            },
        );
    }
}
