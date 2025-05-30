use std::{any::TypeId, collections::HashMap};

use any_handle::AnyHandle;
use commands::{CommandSender, CommandsReceiver};

mod param;
mod system;
pub mod system_fn;

pub mod prelude {
    pub use crate::param::*;
    pub use crate::system::*;
    pub use crate::{Dust, Resource};
    pub use crate::commands::{Commands, Command};
}

mod any_handle;

#[derive(Default)]
pub struct Resources(HashMap<TypeId, AnyHandle>);

impl Resources {
    pub fn insert<R: Resource>(&mut self, value: R) {
        self.0.insert(TypeId::of::<R>(), AnyHandle::new_any(value));
    }

    pub fn handle<R: Resource>(&self) -> Option<AnyHandle<R>> {
        let wrapped = self.0.get(&TypeId::of::<R>())?;
        // Safety: The type is guaranteed to be R
        let read = unsafe { wrapped.clone().unchecked_downcast::<R>() };

        Some(read)
    }
}

pub struct Dust {
    pub resources: Resources,
    commands_buf: CommandsReceiver,
    commands_sx: CommandSender,
}

impl Dust {
    pub fn tick_commands(&mut self) {
        loop {
            match self.commands_buf.0.try_recv() {
                Ok(command) => command.apply(self),
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    eprintln!("WARNING: Commands buffer disconnected");
                    return;
                }
            };
        }
    }
}

impl Default for Dust {
    fn default() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel::<commands::DynCommand>();

        Self {
            resources: Resources::default(),
            commands_buf: CommandsReceiver::new(receiver),
            commands_sx: CommandSender::new(sender),
        }
    }
}

pub trait Resource: Sized + Send + Sync + 'static {}

#[allow(dead_code)]
fn dust_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Dust>();
}

pub mod commands;

pub mod error {
    use std::{fmt::Display, panic::Location};

    #[derive(Debug)]
    pub struct DustUnknownError {
        inner: Box<dyn std::error::Error + Send + Sync + 'static>,
        location: &'static Location<'static>,
    }

    impl DustUnknownError {
        #[track_caller]
        #[inline]
        pub fn new<E: std::error::Error + Send + Sync + 'static>(value: E) -> Self {
            Self {
                inner: Box::new(value),
                location: Location::caller(),
            }
        }
    }

    impl Display for DustUnknownError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "({}:{}:{}): {}",
                self.location.file(),
                self.location.line(),
                self.location.column(),
                self.inner
            )
        }
    }
}
