mod any_handle;
pub mod commands;
pub mod dust;
mod param;
mod resources;
mod system;
pub mod system_fn;

pub mod prelude {
    pub use crate::commands::{Command, Commands};
    pub use crate::dust::{Dust, ConfigureDust};
    pub use crate::param::*;
    pub use crate::resources::Resource;
    pub use crate::system::*;
    pub use crate::schedule::Startup;
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

pub mod schedule {
    use crate::{
        error::DustUnknownError, prelude::Resource, system::{DynSystem, IntoSystem, System}
    };

    pub trait ScheduleLabel: Copy + Clone + Send + Sync + 'static {
        type SystenIn;
        type SystemOut;
    }

    pub struct ScheduledSystems<I, O>(Vec<DynSystem<I, O>>);

    impl<I, O> ScheduledSystems<I, O> {
        pub fn add_system<Marker>(
            &mut self,
            system: impl IntoSystem<Marker, System: System<In = I, Out = O>>,
        ) {
            self.0.push(Box::new(system.into_system()));
        }

        #[inline]
        pub fn take_systems(self) -> impl Iterator<Item = DynSystem<I, O>> {
            self.0.into_iter()
        }
    }

    impl<I, O> Default for ScheduledSystems<I, O> {
        fn default() -> Self {
            Self(Vec::new())
        }
    }

    pub struct LabeledScheduleSystem<S: ScheduleLabel> {
        pub schedule: ScheduledSystems<S::SystenIn, S::SystemOut>,
    }

    impl<S: ScheduleLabel> Resource for LabeledScheduleSystem<S> {}

    impl<S: ScheduleLabel> Default for LabeledScheduleSystem<S> {
        fn default() -> Self {
            Self {
                schedule: ScheduledSystems::default(),
            }
        }
    }

    #[derive(Copy, Clone)]
    pub struct Startup;
    impl ScheduleLabel for Startup {
        type SystenIn = ();
        type SystemOut = Result<(), DustUnknownError>;
    }
}
