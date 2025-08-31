pub mod prelude {
    pub use crate::app::{AppBuilder, ConfigureApp};
    pub use lump_core::error::LumpUnknownError;
    pub use lump_core::prelude::*;

    pub use crate::run::Run;
    pub use crate::startup::Startup;

    pub use crate::plugin::Plugin;

    pub use crate::runtime::{LockingGateway, SystemPermit, SystemReserver};
}

/// Set of exports that will probaly needed for creating the app
pub mod setup {
    pub use crate::async_executor::{AsyncExecutorabel, tokio::TokioRt};
    pub use crate::run::runtime;
    pub use crate::runtime::RuntimeCfg;
}

pub mod app;
mod async_executor;
pub mod plugin;
mod runtime;

pub mod assets;

mod run;
mod startup;
