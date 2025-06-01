mod any_handle;
pub mod commands;
pub mod dust;
mod param;
mod resources;
mod system;
pub mod system_fn;

pub mod prelude {
    pub use crate::commands::{Command, Commands};
    pub use crate::dust::Dust;
    pub use crate::param::*;
    pub use crate::resources::Resource;
    pub use crate::system::*;
}

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

    impl<E> From<E> for DustUnknownError
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        #[track_caller]
        #[inline]
        fn from(value: E) -> Self {
            Self::new(value)
        }
    }

    pub fn panic(e: &DustUnknownError) -> ! {
        panic!("{}", e);
    }
}
