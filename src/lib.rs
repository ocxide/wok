pub mod prelude {
    pub use crate::app::{App, ConfigureApp};
    pub use lump_core::error::LumpUnknownError;
    pub use lump_core::prelude::*;

    pub use crate::run::Run;
    pub use crate::startup::Startup;

    pub use crate::plugin::Plugin;

    pub use crate::runtime::{LockingGateway, SystemPermit, SystemReserver};
}

/// Set of exports that will probaly needed for creating the app
pub mod setup {
    #[cfg(feature = "tokio")]
    pub use crate::async_executor::tokio::TokioRt;
    pub use crate::run::{runtime, DefaultPlugins};
    pub use crate::runtime::RuntimeCfg;
    pub use lump_core::async_executor::AsyncExecutorabel;
    pub use lump_core::error::MainError;
}

pub mod app;
mod async_executor;
pub mod plugin;
mod runtime;

mod run;
mod startup;
