pub use storages::*;

pub trait ScheduleLabel: Send + Sync + 'static {}

pub trait ScheduleConfigure<T, Marker> {
    fn add(self, world: &mut crate::world::World, thing: T);
}

mod storages {
    use std::collections::HashMap;

    use crate::{
        system::{DynSystem, SystemInput},
        system_locking::TaskSystemEntry,
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

    pub struct Systems<
        In: SystemInput + 'static,
        Out: Send + Sync + 'static,
        Meta: Send + Sync + 'static = (),
    >(pub Vec<(TaskSystemEntry<In, Out>, Meta)>);

    impl<In: SystemInput + 'static, Out: Send + Sync + 'static, Meta: Send + Sync + 'static> Default
        for Systems<In, Out, Meta>
    {
        fn default() -> Self {
            Self(Vec::new())
        }
    }

    impl<In: SystemInput + 'static, Out: Send + Sync + 'static, Meta: Send + Sync + 'static>
        Systems<In, Out, Meta>
    {
        #[inline]
        pub fn add(&mut self, entry: TaskSystemEntry<In, Out>, meta: Meta) {
            self.0.push((entry, meta));
        }

        pub fn iter(&self) -> impl Iterator<Item = &(TaskSystemEntry<In, Out>, Meta)> {
            self.0.iter()
        }

        pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (TaskSystemEntry<In, Out>, Meta)> {
            self.0.iter_mut()
        }
    }
}
