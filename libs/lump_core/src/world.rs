use std::sync::mpsc::Sender;

use crate::any_handle::AnyHandle;
use crate::commands::{self, CommandSender, CommandsReceiver};
use crate::prelude::Resource;
use crate::resources::{LocalResource, LocalResources, Resources};
use crate::schedule::{ScheduleConfigure, ScheduleLabel};
use crate::system::{IntoSystem, System};

pub use access::SystemLock;
pub use meta::SystemId;

pub(crate) mod access {
    use core::panic;
    use std::{
        collections::{HashMap, HashSet},
        num::NonZero,
    };

    use crate::resources::ResourceId;

    #[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
    pub enum AccessMode {
        Read,
        Write,
    }

    #[derive(Debug)]
    pub enum WorldAccess {
        Read(NonZero<usize>),
        Write,
    }

    #[derive(Default)]
    pub struct WorldLocks {
        resources: HashMap<ResourceId, WorldAccess>,
    }

    impl WorldLocks {
        /// # Safety
        /// Caller must ensure the access is valid, since it would leave an inconsistent state
        pub unsafe fn do_access(&mut self, access: &SystemLock) {
            for &(resource_id, mode) in access.resources.iter() {
                self.resources
                    .entry(resource_id)
                    .and_modify(|access| {
                        // We know the access is read because we checked above
                        if let WorldAccess::Read(count) = access {
                            *count = NonZero::new(count.get() + 1).unwrap();
                        }
                    })
                    .or_insert_with(|| match mode {
                        AccessMode::Read => WorldAccess::Read(NonZero::new(1).unwrap()),
                        AccessMode::Write => WorldAccess::Write,
                    });
            }
        }

        pub fn try_access(&mut self, access: &SystemLock) -> Result<(), ()> {
            if !self.can_lock(access) {
                return Err(());
            }

            // Safety: we know the access is valid
            unsafe {
                self.do_access(access);
            }

            Ok(())
        }

        pub fn release_access(&mut self, access: &SystemLock) {
            for (resource_id, mode) in access.resources.iter() {
                let Some(access) = self.resources.get_mut(resource_id) else {
                    continue;
                };

                match (access, mode) {
                    (WorldAccess::Read(count), AccessMode::Read) => {
                        let value = NonZero::new(count.get() - 1);

                        if let Some(value) = value {
                            *count = value;
                        } else {
                            self.resources.remove(resource_id);
                        }
                    }
                    (WorldAccess::Write, AccessMode::Write) => {
                        self.resources.remove(resource_id);
                    }
                    (world, system) => {
                        panic!("access are not compatible {:?} != {:?}", world, system)
                    }
                }
            }
        }

        pub(crate) fn can_lock(&self, access: &SystemLock) -> bool {
            for (resource_id, mode) in access.resources.iter() {
                let access = self.resources.get(resource_id);
                let allow = matches!(
                    (access, mode),
                    (None, _) | (Some(WorldAccess::Read(_)), AccessMode::Read)
                );

                if !allow {
                    return false;
                }
            }

            true
        }
    }

    #[derive(Default)]
    pub struct SystemLock {
        resources: HashSet<(ResourceId, AccessMode)>,
    }

    pub enum AlreadyRegistered {
        Read,
        Write,
    }

    impl SystemLock {
        pub fn has_resource_write(&self, resource: ResourceId) -> bool {
            self.resources.contains(&(resource, AccessMode::Write))
        }

        pub fn has_resource_read(&self, resource: ResourceId) -> bool {
            self.resources.contains(&(resource, AccessMode::Read))
        }

        pub fn register_resource_read(
            &mut self,
            resource: ResourceId,
        ) -> Result<(), AlreadyRegistered> {
            if self.has_resource_write(resource) {
                return Err(AlreadyRegistered::Write);
            }

            self.resources.insert((resource, AccessMode::Read));
            Ok(())
        }

        pub fn register_resource_write(
            &mut self,
            resource: ResourceId,
        ) -> Result<(), AlreadyRegistered> {
            if self.has_resource_read(resource) {
                return Err(AlreadyRegistered::Read);
            }

            self.resources.insert((resource, AccessMode::Write));
            Ok(())
        }
    }
}

pub(crate) mod meta {
    use std::num::NonZero;

    use super::access::SystemLock;

    #[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
    pub struct SystemId(NonZero<usize>);

    impl SystemId {
        pub const fn local(self) -> LocalSystemId {
            LocalSystemId(self.0)
        }
    }

    pub struct LocalSystemId(NonZero<usize>);

    impl LocalSystemId {
        /// Asume the system id is valid for this world
        pub fn cast_global(self) -> SystemId {
            SystemId(self.0)
        }
    }

    #[derive(Debug, Default)]
    pub struct SystemsMeta<D>(Vec<D>);

    impl<D> SystemsMeta<D> {
        pub fn add(&mut self, meta: D) -> LocalSystemId {
            self.0.push(meta);
            LocalSystemId(NonZero::new(self.0.len()).expect("vec len to be nonzero"))
        }

        pub fn get(&self, id: LocalSystemId) -> Option<&D> {
            self.0.get(id.0.get() - 1)
        }
    }

    #[derive(Default)]
    pub struct SystemsRw(SystemsMeta<SystemLock>);

    impl SystemsRw {
        #[inline]
        pub fn add(&mut self, access: SystemLock) -> SystemId {
            self.0.add(access).cast_global()
        }

        #[inline]
        pub fn get(&self, id: SystemId) -> Option<&SystemLock> {
            self.0.get(id.local())
        }
    }
}

#[derive(Debug)]
pub enum WorldSystemLockError {
    NotRegistered,
    InvalidAccess,
}

pub struct SystemFinisher {
    systemid: SystemId,
    sender: Sender<SystemId>,
}

impl SystemFinisher {
    pub fn mark_finished(self) -> Result<(), std::sync::mpsc::SendError<SystemId>> {
        self.sender.send(self.systemid)
    }
}

pub struct WorldState {
    pub resources: Resources,
    pub(crate) commands_sx: CommandSender,
}

impl WorldState {
    /// # Panics
    /// Panics if the resource is not found
    pub fn get_resource<R: Resource>(&self) -> AnyHandle<R> {
        let handle = self.resources.handle();
        if let Some(handle) = handle {
            handle
        } else {
            panic!("Resource with type `{}` not found", std::any::type_name::<R>())
        }
    }

    #[inline]
    pub fn try_take_resource<R: Resource>(&mut self) -> Option<R> {
        self.resources.try_take()
    }
}

#[derive(Default)]
pub struct SystemLocks {
    rw: access::WorldLocks,
    systems_rw: meta::SystemsRw,
}

impl SystemLocks {
    pub fn try_lock(&mut self, systemid: SystemId) -> Result<(), WorldSystemLockError> {
        let rw = self
            .systems_rw
            .get(systemid)
            .ok_or(WorldSystemLockError::NotRegistered)?;

        self.rw
            .try_access(rw)
            .map_err(|_| WorldSystemLockError::InvalidAccess)?;

        Ok(())
    }

    pub fn release(&mut self, systemid: SystemId) {
        let Some(rw) = self.systems_rw.get(systemid) else {
            return;
        };

        self.rw.release_access(rw);
    }

    /// # Safety
    /// Caller must ensure the access is valid
    pub unsafe fn lock_rw(&mut self, rw: &access::SystemLock) {
        unsafe {
            self.rw.do_access(rw);
        }
    }

    pub fn can_lock_rw(&self, rw: &access::SystemLock) -> bool {
        self.rw.can_lock(rw)
    }

    pub fn try_lock_rw(&mut self, rw: &access::SystemLock) -> Result<(), WorldSystemLockError> {
        self.rw.try_access(rw).map_err(|_| WorldSystemLockError::InvalidAccess)?;
        Ok(())
    }

    pub fn release_rw(&mut self, rw: &access::SystemLock) {
        self.rw.release_access(rw);
    }
}

pub struct WorldCenter {
    pub(crate) commands_rx: CommandsReceiver,
    pub resources: LocalResources,
    pub system_locks: SystemLocks,
}

impl WorldCenter {
    pub fn tick_commands(&mut self, state: &mut WorldState) {
        for command in self.commands_rx.recv() {
            command.apply(WorldMut {
                state,
                center: &mut self.resources,
            });
        }
    }
}

pub struct WorldMut<'w> {
    pub state: &'w mut WorldState,
    pub center: &'w mut LocalResources,
}

pub struct World {
    pub state: WorldState,
    pub center: WorldCenter,
}

impl Default for World {
    fn default() -> Self {
        let (sender, receiver) = commands::commands();

        Self {
            center: WorldCenter {
                system_locks: SystemLocks::default(),
                commands_rx: receiver,
                resources: LocalResources::default(),
            },
            state: WorldState {
                resources: Resources::default(),
                commands_sx: sender,
            },
        }
    }
}

impl World {
    pub fn register_system(&mut self, system: &impl System) -> SystemId {
        let mut rw = SystemLock::default();
        system.init(&mut rw);

        self.center.system_locks.systems_rw.add(rw)
    }

    pub fn into_parts(self) -> (WorldState, WorldCenter) {
        (self.state, self.center)
    }
}

#[test]
fn world_is_send() {
    fn assert_send<T: Send + Sync + 'static>() {}
    assert_send::<WorldState>();
}

pub trait ConfigureWorld: Sized {
    fn world_mut(&mut self) -> &mut World;
    fn world(&self) -> &World;

    fn init_resource<R: Resource + Default>(mut self) -> Self {
        self.world_mut().state.resources.init::<R>();
        self
    }

    fn insert_resource<R: Resource>(mut self, resource: R) -> Self {
        self.world_mut().state.resources.insert(resource);
        self
    }

    fn insert_local_resource<R: LocalResource>(mut self, resource: R) -> Self {
        self.world_mut().center.resources.insert(resource);
        self
    }

    fn add_system<Sch, S, SMarker>(mut self, _: Sch, system: S) -> Self
    where
        Sch: ScheduleLabel
            + ScheduleConfigure<<S::System as System>::In, <S::System as System>::Out>,
        S: IntoSystem<SMarker>,
    {
        let system = system.into_system();
        let id = self.world_mut().register_system(&system);
        Sch::add(self.world_mut(), id, Box::new(system));

        self
    }
}

impl ConfigureWorld for World {
    fn world_mut(&mut self) -> &mut World {
        self
    }
    fn world(&self) -> &World {
        self
    }
}
