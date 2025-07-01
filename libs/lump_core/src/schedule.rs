pub use storages::*;

use crate::system::DynSystem;
use crate::system::SystemInput;

pub trait ScheduleLabel: Copy + Clone + Send + Sync + 'static {}

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
        resources::LocalResource,
        system::{DynSystem, SystemInput},
        world::SystemId,
    };

    pub struct SystemsMap<In: SystemInput + 'static, Out: 'static> {
        systems: HashMap<SystemId, DynSystem<In, Out>>,
    }

    impl<In: SystemInput + 'static, Out: 'static> Default for SystemsMap<In, Out> {
        fn default() -> Self {
            Self {
                systems: HashMap::default(),
            }
        }
    }

    impl<In: SystemInput + 'static, Out: 'static> Resource for SystemsMap<In, Out> {}
    impl<In: SystemInput + 'static, Out: 'static> LocalResource for SystemsMap<In, Out> {}

    impl<In: SystemInput + 'static, Out: 'static> SystemsMap<In, Out> {
        pub fn add_system(&mut self, systemid: SystemId, system: DynSystem<In, Out>) {
            self.systems.insert(systemid, system);
        }

        #[inline]
        pub fn extract_if(
            &mut self,
            mut predicate: impl FnMut(SystemId, &DynSystem<In, Out>) -> bool,
        ) -> impl Iterator<Item = (SystemId, DynSystem<In, Out>)> {
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
    impl<In: SystemInput + 'static, Out: 'static> LocalResource for Systems<In, Out> {}
}
