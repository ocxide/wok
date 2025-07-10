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
    use std::collections::HashMap;

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
        pub fn remove_system(&mut self, systemid: SystemId) {
            self.systems.remove(&systemid);
        }

        #[inline]
        pub fn is_empty(&self) -> bool {
            self.systems.is_empty()
        }

        #[inline]
        pub fn get(&self, systemid: SystemId) -> Option<&DynSystem<In, Out>> {
            self.systems.get(&systemid)
        }
    }

    pub struct Systems<In: SystemInput + 'static, Out: 'static, Meta: Send + Sync + 'static = ()>(
        pub Vec<(SystemId, DynSystem<In, Out>, Meta)>,
    );

    impl<In: SystemInput + 'static, Out: 'static, Meta: Send + Sync + 'static> Default for Systems<In, Out, Meta> {
        fn default() -> Self {
            Self(Vec::new())
        }
    }

    impl<In: SystemInput + 'static, Out: 'static, Meta: Send + Sync + 'static> Systems<In, Out, Meta> {
        #[inline]
        pub fn add(&mut self, systemid: SystemId, system: DynSystem<In, Out>, meta: Meta) {
            self.0.push((systemid, system, meta));
        }

        pub fn iter_mut(
            &mut self,
        ) -> impl Iterator<Item = (SystemId, &mut DynSystem<In, Out>, &mut Meta)> {
            self.0
                .iter_mut()
                .map(|(id, system, meta)| (*id, system, meta))
        }
    }

    impl<In: SystemInput + 'static, Out: 'static, Meta: Send + Sync + 'static> Resource
        for Systems<In, Out, Meta>
    {
    }
    impl<In: SystemInput + 'static, Out: 'static, Meta: Send + Sync + 'static> LocalResource
        for Systems<In, Out, Meta>
    {
    }
}
