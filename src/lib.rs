pub mod prelude {
    pub use crate::app::{AppBuilder, ConfigureMoreWorld};
    pub use lump_core::error::LumpUnknownError;
    pub use lump_core::prelude::*;

    pub use crate::events::{Event, Events, OnEvents, EventSender};
    pub use crate::startup::Startup;

    pub use crate::plugin::Plugin;
}

pub mod app;
mod events;
pub mod foreign;
pub mod plugin;
pub(crate) mod runtime;
mod startup;

pub mod assets;
