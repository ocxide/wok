pub mod prelude {
    pub use crate::app::{AppBuilder, ConfigureMoreWorld};
    pub use lump_core::error::LumpUnknownError;
    pub use lump_core::prelude::*;

    pub use crate::startup::Startup;

    pub use crate::plugin::Plugin;

    pub use crate::locks_runtime::{LockingGateway, SystemPermit, SystemReserver};
}

pub mod app;
mod async_runtime;
pub(crate) mod locks_runtime;
pub mod plugin;
mod startup;

pub mod assets;
