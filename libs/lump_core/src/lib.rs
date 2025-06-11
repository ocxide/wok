mod any_handle;
pub mod commands;
mod local_any_handle;
mod param;
mod resources;
mod system;
pub mod system_fn;
pub mod world;

pub mod prelude {
    pub use crate::commands::{Command, Commands};
    pub use crate::param::*;
    pub use crate::resources::Resource;
    pub use crate::system::*;
    pub use crate::world::{ConfigureWorld, World, WorldState};
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

    pub fn panic(e: &LumpUnknownError) -> ! {
        panic!("{}", e);
    }
}

pub mod schedule {
    use hashbrown::HashMap;

    use crate::{prelude::Resource, system::DynSystem, world::meta::SystemId};

    pub trait ScheduleLabel: Copy + Clone + Send + Sync + 'static {
        type SystenIn;
        type SystemOut;
    }

    pub struct ScheduledSystems<I, O> {
        systems: HashMap<SystemId, DynSystem<I, O>>,
    }

    impl<I, O> ScheduledSystems<I, O> {
        pub fn add_system(&mut self, systemid: SystemId, system: DynSystem<I, O>) {
            self.systems.insert(systemid, system);
        }

        #[inline]
        pub fn extract_if(
            &mut self,
            mut predicate: impl FnMut(SystemId, &DynSystem<I, O>) -> bool,
        ) -> impl Iterator<Item = (SystemId, DynSystem<I, O>)> {
            self.systems
                .extract_if(move |systemid, system| predicate(*systemid, system))
        }

        #[inline]
        pub fn remove_system(&mut self, systemid: SystemId) {
            self.systems.remove(&systemid);
        }

        #[inline]
        pub fn is_empty(&self) -> bool {
            self.systems.is_empty()
        }
    }

    impl<I, O> Default for ScheduledSystems<I, O> {
        fn default() -> Self {
            Self {
                systems: Default::default(),
            }
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
}
