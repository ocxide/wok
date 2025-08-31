use lump_core::{
    error::LumpUnknownError,
    prelude::{Commands, Param, ResMut, Resource},
};

pub use loaders::*;

pub struct AssetInit<'r, R: Resource> {
    commands: Commands<'r>,
    _marker: std::marker::PhantomData<fn(R)>,
}

impl<'r, R: Resource> Param for AssetInit<'r, R> {
    type Owned = <Commands<'r> as Param>::Owned;
    type AsRef<'p> = AssetInit<'p, R>;

    fn init(rw: &mut lump_core::world::SystemLock) {
        ResMut::<R>::init(rw);
    }

    fn get(world: &lump_core::prelude::WorldState) -> Self::Owned {
        <Commands<'r> as Param>::get(world)
    }

    fn from_owned(owned: &Self::Owned) -> Self::AsRef<'_> {
        AssetInit {
            commands: <Commands<'r> as Param>::from_owned(owned),
            _marker: std::marker::PhantomData,
        }
    }

    fn get_ref(world: &lump_core::prelude::WorldState) -> Self::AsRef<'_> {
        AssetInit {
            commands: <Commands<'r> as Param>::get_ref(world),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'r, R: Resource> AssetInit<'r, R> {
    pub async fn with(mut self, loader: impl AssetLoader<R>) -> Result<(), LumpUnknownError> {
        let resource = loader.load().await?;
        self.commands.insert_resource(resource);

        Ok(())
    }
}

pub trait AssetLoader<T> {
    fn load(self) -> impl Future<Output = Result<T, LumpUnknownError>>;
}

pub mod loaders {
    use std::path::Path;

    use lump_core::error::LumpUnknownError;

    use super::AssetLoader;

    pub struct TomlLoader<P: AsRef<Path> + 'static + Send>(pub P);

    impl<T, P> AssetLoader<T> for TomlLoader<P>
    where
        T: serde::de::DeserializeOwned,
        P: AsRef<Path> + Send,
    {
        async fn load(self) -> Result<T, LumpUnknownError> {
            let buf = tokio::task::spawn_blocking(move || std::fs::read_to_string(self.0.as_ref()))
                .await??;

            toml::from_str(&buf).map_err(Into::into)
        }
    }
}
