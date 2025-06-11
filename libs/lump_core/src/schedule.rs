pub use storages::HomogenousScheduleSystem;

pub trait ScheduleLabel: Copy + Clone + Send + Sync + 'static {
    type SystenIn;
    type SystemOut;
}

mod storages {
    use hashbrown::HashMap;

    use crate::{prelude::Resource, system::DynSystem, world::SystemId};

    use super::ScheduleLabel;

    pub struct HomogenousScheduleSystem<S: ScheduleLabel> {
        systems: HashMap<SystemId, DynSystem<S::SystenIn, S::SystemOut>>,
    }

    impl<S: ScheduleLabel> Resource for HomogenousScheduleSystem<S> {}

    impl<S: ScheduleLabel> Default for HomogenousScheduleSystem<S> {
        fn default() -> Self {
            Self {
                systems: Default::default(),
            }
        }
    }

    impl<S: ScheduleLabel> HomogenousScheduleSystem<S> {
        pub fn add_system(&mut self, systemid: SystemId, system: DynSystem<S::SystenIn, S::SystemOut>) {
            self.systems.insert(systemid, system);
        }

        #[inline]
        pub fn extract_if(
            &mut self,
            mut predicate: impl FnMut(SystemId, &DynSystem<S::SystenIn, S::SystemOut>) -> bool,
        ) -> impl Iterator<Item = (SystemId, DynSystem<S::SystenIn, S::SystemOut>)> {
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
}
