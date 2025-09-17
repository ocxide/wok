pub mod prelude {
    pub use crate::app::{App, ConfigureApp};
    pub use wok_core::error::WokUnknownError;
    pub use wok_core::prelude::*;

    pub use crate::run::Run;
    pub use crate::startup::Startup;

    pub use crate::plugin::Plugin;
}

/// Set of exports that will probaly needed for creating the app
pub mod setup {
    #[cfg(feature = "tokio")]
    pub use crate::async_executor::tokio::TokioRt;
    pub use crate::run::{DefaultPlugins, runtime};
    pub use crate::runtime::RuntimeCfg;
    pub use wok_core::async_executor::AsyncExecutorabel;
    pub use wok_core::error::MainError;
}

pub mod app;
mod async_executor;
pub mod plugin;
mod runtime;

mod run;
mod startup;

pub mod remote_gateway;
