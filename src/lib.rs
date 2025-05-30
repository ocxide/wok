use std::{any::TypeId, collections::HashMap};

use any_handle::AnyHandle;

mod param;
mod system;
pub mod system_fn;

pub mod prelude {
    pub use crate::param::*;
    pub use crate::system::*;
    pub use crate::{Dust, Resource};
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

#[derive(Default)]
pub struct Dust {
    pub resources: Resources,
}

pub trait Resource: Sized + Send + Sync + 'static {}

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
