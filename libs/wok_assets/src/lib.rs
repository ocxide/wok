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
}

pub trait AssetOrigin<T>: Send + Sync + Clone {
    fn load(self) -> Result<T, WokUnknownError>;
}

pub mod origins {
    use std::path::Path;

    use wok::prelude::WokUnknownError;

    use super::AssetOrigin;

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
        fn load(self) -> Result<T, WokUnknownError> {
            let buf = std::fs::read_to_string(self.0.as_ref())?;

            toml::from_str(&buf).map_err(Into::into)
        }
    }
}

pub trait AssetsCollection: Sized + 'static {
    type Assets: BorrowMutParam;
    fn insert_all(self, commands: &mut Commands);
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

    pub fn assets<A: AssetsCollection>(self) -> AssetsPlugin<O, A>
    where
        O: AssetOrigin<A>,
    {
        AssetsPlugin {
            origin: self.origin,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<O: AssetOrigin<A> + 'static, A: for<'a> AssetsCollection<Assets: Param<AsRef<'a> = A::Assets>>>
    Plugin for AssetsPlugin<O, A>
{
    fn setup(self, app: &mut wok::prelude::App) {
        use wok::prelude::ConfigureWorld;
        let origin = self.origin;
        app.add_systems(Startup, move |mut init: AssetsCollectionInit<'_, A>| {
            init.load(origin.clone())
        });
    }
}
