pub use storages::HomogenousSchedule;
pub use storages::HomogenousScheduleSystem;
pub use storages::Systems;

use crate::system::DynSystem;
use crate::system::SystemInput;

pub trait ScheduleLabel: Copy + Clone + Send + Sync + 'static {
    fn init(world: &mut crate::world::World);
}

pub trait ScheduleConfigure<In: SystemInput, Out> {
    fn add(
        world: &mut crate::world::World,
        systemid: crate::world::SystemId,
        system: DynSystem<In, Out>,
    );
}

mod storages {
    use hashbrown::HashMap;

    use crate::{
        prelude::Resource,
        system::{DynSystem, SystemInput},
        world::SystemId,
    };

    use super::ScheduleLabel;

    pub trait HomogenousSchedule: ScheduleLabel {
        type SystenIn;
        type SystemOut;
    }

    pub struct HomogenousScheduleSystem<S: HomogenousSchedule> {
        systems: HashMap<SystemId, DynSystem<S::SystenIn, S::SystemOut>>,
    }

    impl<S: HomogenousSchedule> Resource for HomogenousScheduleSystem<S> {}

    impl<S: HomogenousSchedule> Default for HomogenousScheduleSystem<S> {
        fn default() -> Self {
            Self {
                systems: Default::default(),
            }
        }
    }

    impl<S: HomogenousSchedule> HomogenousScheduleSystem<S> {
        pub fn add_system(
            &mut self,
            systemid: SystemId,
            system: DynSystem<S::SystenIn, S::SystemOut>,
        ) {
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

    pub struct Systems<In: SystemInput + 'static, Out: 'static>(
        pub Vec<(SystemId, DynSystem<In, Out>)>,
    );

    impl<In: SystemInput + 'static, Out: 'static> Systems<In, Out> {
        #[inline]
        pub fn add(&mut self, systemid: SystemId, system: DynSystem<In, Out>) {
            self.0.push((systemid, system));
        }
    }

    impl<In: SystemInput + 'static, Out: 'static> Resource for Systems<In, Out> {}
}
