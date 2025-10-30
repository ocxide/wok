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
    pub use crate::error::{LabelledError, WokUnknownError};
    pub use crate::param::*;
    pub use crate::resources::{Immutable, Mutable, Resource};
    pub use crate::system::*;
    pub use crate::world::{
        ConfigureWorld, SystemLock, UnsafeMutState, UnsafeWorldState, World, WorldState,
    };
    pub use wok_derive::Param;
}

pub mod error {
    use std::{fmt::Display, panic::Location};

    #[derive(Debug, thiserror::Error)]
    #[error("`{label}`: {error}")]
    pub struct LabelledError<E> {
        pub error: E,
        pub label: &'static str,
    }

    impl<E> LabelledError<E> {
        pub fn new(label: &'static str, error: E) -> Self {
            Self { error, label }
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("{0}")]
    pub struct MessageError(pub std::borrow::Cow<'static, str>);

    #[derive(Debug)]
    pub struct WokUnknownError {
        inner: Box<dyn std::error::Error + Send + Sync + 'static>,
        location: &'static Location<'static>,
    }

    impl WokUnknownError {
        #[track_caller]
        #[inline]
        pub fn new<E: std::error::Error + Send + Sync + 'static>(value: E) -> Self {
            Self {
                inner: Box::new(value),
                location: Location::caller(),
            }
        }

        #[track_caller]
        #[inline]
        pub fn from_message(error: impl Into<std::borrow::Cow<'static, str>>) -> Self {
            Self::new(MessageError(error.into()))
        }
    }

    impl Display for WokUnknownError {
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

    impl<E> From<E> for WokUnknownError
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        #[track_caller]
        #[inline]
        fn from(value: E) -> Self {
            Self::new(value)
        }
    }

    pub struct MainError(pub WokUnknownError);
    impl std::fmt::Debug for MainError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            <WokUnknownError as Display>::fmt(&self.0, f)
        }
    }

    impl From<WokUnknownError> for MainError {
        fn from(value: WokUnknownError) -> Self {
            MainError(value)
        }
    }
}

pub mod schedule;
