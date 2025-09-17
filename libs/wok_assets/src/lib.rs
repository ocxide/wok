use wok_core::{
    error::WokUnknownError,
    prelude::{Commands, Param, Resource},
};

pub use loaders::*;

#[derive(Param)]
#[param(usage = lib)]
pub struct AssetInit<'r, R: Resource> {
    commands: Commands<'r>,
    #[param(default)]
    _marker: std::marker::PhantomData<fn(R)>,
}

impl<'r, R: Resource> AssetInit<'r, R> {
    pub async fn with(mut self, loader: impl AssetLoader<R>) -> Result<(), WokUnknownError> {
        let resource = loader.load().await?;
        self.commands.insert_resource(resource);

        Ok(())
    }
}

pub trait AssetLoader<T> {
    fn load(self) -> impl Future<Output = Result<T, WokUnknownError>>;
}

pub mod loaders {
    use std::path::Path;

    use wok_core::error::WokUnknownError;

    use super::AssetLoader;

    pub struct TomlLoader<P: AsRef<Path> + 'static + Send>(pub P);

    impl<T, P> AssetLoader<T> for TomlLoader<P>
    where
        T: serde::de::DeserializeOwned,
        P: AsRef<Path> + Send,
    {
        async fn load(self) -> Result<T, WokUnknownError> {
            #[cfg(feature = "tokio")]
            let buf = tokio::task::spawn_blocking(move || std::fs::read_to_string(self.0.as_ref()))
                .await??;

            // TODO: make this async
            #[cfg(not(feature = "tokio"))]
            let buf = std::fs::read_to_string(self.0.as_ref())?;

            toml::from_str(&buf).map_err(Into::into)
        }
    }
}
