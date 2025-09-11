mod any_handle;
pub mod commands;
mod param;
pub mod resources;
mod system;
pub mod system_fn;
pub mod world;

pub mod async_executor;
pub mod runtime;

pub mod prelude {
    pub use crate::commands::{Command, Commands};
    pub use crate::error::LumpUnknownError;
    pub use crate::param::*;
    pub use crate::resources::Resource;
    pub use crate::system::*;
    pub use crate::world::{ConfigureWorld, World, WorldState, UnsafeWorldState};
    pub use lump_derive::Param;
}

pub mod error {
    use std::{fmt::Display, panic::Location};

    #[derive(Debug)]
    pub struct LumpUnknownError {
        inner: Box<dyn std::error::Error + Send + Sync + 'static>,
        location: &'static Location<'static>,
    }

    impl LumpUnknownError {
        #[track_caller]
        #[inline]
        pub fn new<E: std::error::Error + Send + Sync + 'static>(value: E) -> Self {
            Self {
                inner: Box::new(value),
                location: Location::caller(),
            }
        }
    }

    impl Display for LumpUnknownError {
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

    impl<E> From<E> for LumpUnknownError
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        #[track_caller]
        #[inline]
        fn from(value: E) -> Self {
            Self::new(value)
        }
    }

    pub struct MainError(pub LumpUnknownError);
    impl std::fmt::Debug for MainError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            <LumpUnknownError as Display>::fmt(&self.0, f)
        }
    }

    impl From<LumpUnknownError> for MainError {
        fn from(value: LumpUnknownError) -> Self {
            MainError(value)
        }
    }
}

pub mod schedule;
