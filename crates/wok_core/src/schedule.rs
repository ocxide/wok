pub use storages::*;

/// Stores
pub trait ConfigureObjects<O, Marker> {
    fn add_objs(self, world: &mut crate::world::World, objs: O);
}

pub trait ScheduleLabel: Send + Sync {}

pub trait ScheduleConfigure<T, Marker> {
    fn add(self, world: &mut crate::world::World, thing: T);
}

mod storages {
    use std::collections::HashMap;

    use crate::{
        system::{DynTaskSystem, SystemInput},
        world::{SystemId, gateway::TaskSystemEntry},
    };

    pub struct SystemsMap<In: SystemInput + 'static, Out: 'static> {
        systems: HashMap<SystemId, DynTaskSystem<In, Out>>,
    }

    impl<In: SystemInput + 'static, Out: 'static> Default for SystemsMap<In, Out> {
        fn default() -> Self {
            Self {
                systems: HashMap::default(),
            }
        }
    }

    impl<In: SystemInput + 'static, Out: 'static> SystemsMap<In, Out> {
        pub fn add_system(&mut self, systemid: SystemId, system: DynTaskSystem<In, Out>) {
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
        pub fn get(&self, systemid: SystemId) -> Option<&DynTaskSystem<In, Out>> {
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

pub mod dependency_graph {
    use std::collections::HashMap;

    use crate::{
        resources::ResourceId,
        world::{SystemId, access::AccessMode, meta::SystemsRw},
    };

    /// Signals whenever system `A` (key) requires system `B` mutation (value) to be executed before it.
    #[derive(Default)]
    pub struct SystemsMutationDependencyGraph(HashMap<SystemId, Vec<SystemId>>);

    impl SystemsMutationDependencyGraph {
        pub fn get_dependencies(&self, system: SystemId) -> Option<&[SystemId]> {
            self.0.get(&system).map(|v| v.as_slice())
        }
    }

    #[derive(Debug)]
    pub enum DependencyGraphError {
        SystemNotRegistered,
    }

    pub fn build_sequencial_graph(
        systems: &[SystemId],
        rw: &SystemsRw,
    ) -> Result<SystemsMutationDependencyGraph, DependencyGraphError> {
        let mut graph = SystemsMutationDependencyGraph(HashMap::with_capacity(systems.len()));
        let mut write_dependencies = HashMap::<ResourceId, SystemId>::new();

        for system in systems {
            let rw = rw
                .get(*system)
                .ok_or(DependencyGraphError::SystemNotRegistered)?;

            let depend_on_systems = rw
                .entries()
                .map(|e| &e.0)
                .filter_map(|id| write_dependencies.get(id))
                .cloned()
                .collect::<Vec<_>>();

            if !depend_on_systems.is_empty() {
                graph
                    .0
                    .insert(*system, depend_on_systems);
            }

            let write_access = rw
                .entries()
                .filter(|e| e.1 == AccessMode::Write)
                .map(|e| e.0)
                .map(|id| (id, *system));
            write_dependencies.extend(write_access);
        }

        Ok(graph)
    }

    #[cfg(test)]
    mod tests {
        use crate::{
            param::{Param, Res, ResMut},
            prelude::Resource,
            world::access,
        };

        use super::*;

        #[derive(Resource)]
        #[resource(usage = core, mutable = true)]
        struct MyResA;

        #[derive(Resource)]
        #[resource(usage = core, mutable = true)]
        struct MyResB;

        #[test]
        fn builds() {
            let mut rws = SystemsRw::default();

            let a = rws.add({
                let mut locks = access::SystemLock::default();
                <ResMut<MyResA> as Param>::init(&mut locks);

                locks
            });

            let b = rws.add({
                let mut locks = access::SystemLock::default();
                <(Res<MyResA>, ResMut<MyResB>) as Param>::init(&mut locks);

                locks
            });

            let c = rws.add({
                let mut locks = access::SystemLock::default();
                <Res<MyResB> as Param>::init(&mut locks);

                locks
            });

            let graph = build_sequencial_graph(&[a, b, c], &rws).unwrap();
            assert_eq!(graph.0.get(&a), None);
            assert_eq!(graph.0.get(&b), Some(&vec![a]));
            assert_eq!(graph.0.get(&c), Some(&vec![b]));
        }
    }
}
