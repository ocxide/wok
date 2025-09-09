use crate::commands::{self, CommandSender, CommandsReceiver};
use crate::param::Param;
use crate::prelude::Resource;
use crate::resources::{LocalResource, LocalResources, Resources};
use crate::schedule::{ScheduleConfigure, ScheduleLabel};
use crate::system::System;

pub use access::SystemLock;
pub use meta::SystemId;

pub(crate) mod access;
pub(crate) mod meta;

#[derive(Debug)]
pub enum WorldSystemLockError {
    NotRegistered,
    InvalidAccess,
}

pub struct WorldState {
    pub resources: Resources,
    pub(crate) commands_sx: CommandSender,
}

impl WorldState {
    pub fn get<P: Param>(&mut self) -> P::AsRef<'_> {
        // Safety: by being the olny owner `&mut self`, this is allowed

        unsafe { P::get_ref(self.as_unsafe_world_state()) }
    }

    fn as_unsafe_world_state(&mut self) -> &UnsafeWorldState {
        // Safety: by being the olny owner `&mut self`, this is allowed
        unsafe { &*(self as *const WorldState as *const UnsafeWorldState) }
    }
}

pub use unsafe_world_state::UnsafeWorldState;

mod unsafe_world_state {
    use std::cell::UnsafeCell;

    use crate::{
        any_handle::{Handle, HandleMut},
        commands::CommandSender,
        prelude::Resource,
    };

    use super::WorldState;

    #[repr(transparent)]
    pub struct UnsafeWorldState(UnsafeCell<WorldState>);
    unsafe impl Sync for UnsafeWorldState {}
    unsafe impl Send for UnsafeWorldState {}

    impl UnsafeWorldState {
        /// # Safety
        /// Caller must ensure the access is valid
        pub unsafe fn resource_handle<R: Resource>(&self) -> Option<Handle<R>> {
            unsafe { &*self.0.get() }.resources.handle()
        }

        /// # Safety
        /// Caller must ensure the access is valid
        pub unsafe fn resource_handle_mut<R: Resource>(&self) -> Option<HandleMut<R>> {
            unsafe { &*self.0.get() }.resources.handle_mut()
        }

        /// # Safety
        /// Caller must ensure the access is valid
        pub unsafe fn get_resource<R: Resource>(&self) -> Option<&R> {
            let resources = &unsafe { &*self.0.get() }.resources;
            let handle_ref = resources.handle_ref()?;
            Some(unsafe { handle_ref.get() })
        }

        /// # Safety
        /// Caller must ensure the access is valid
        pub unsafe fn get_resource_mut<R: Resource>(&self) -> Option<&mut R> {
            let resources = &mut unsafe { &mut *self.0.get() }.resources;
            let handle_ref = resources.handle_ref_mut()?;
            Some(unsafe { handle_ref.get_mut() })
        }

        /// # Safety
        /// Caller must ensure the access is valid
        pub unsafe fn take_resource<R: Resource>(&self) -> Option<R> {
            let resources = &mut unsafe { &mut *self.0.get() }.resources;
            resources.try_take()
        }

        pub fn try_take_resource<R: Resource>(&mut self) -> Option<R> {
            self.0.get_mut().resources.try_take()
        }

        pub fn commands(&self) -> CommandSender {
            unsafe { &*self.0.get() }.commands_sx.clone()
        }
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
        self.rw
            .try_access(rw)
            .map_err(|_| WorldSystemLockError::InvalidAccess)?;
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

    pub fn register_system(&mut self, system: &impl System) -> SystemId {
        let mut rw = SystemLock::default();
        system.init(&mut rw);

        self.system_locks.systems_rw.add(rw)
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
    #[inline]
    pub fn register_system(&mut self, system: &impl System) -> SystemId {
        self.center.register_system(system)
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

    fn add_system<Sch, T, Marker>(mut self, schedule: Sch, into_cfg: T) -> Self
    where
        Sch: ScheduleLabel + ScheduleConfigure<T, Marker>,
    {
        schedule.add(self.world_mut(), into_cfg);

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

impl ConfigureWorld for &mut World {
    fn world_mut(&mut self) -> &mut World {
        self
    }
    fn world(&self) -> &World {
        self
    }
}
