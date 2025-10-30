pub mod gateway;

use std::sync::Arc;

use crate::commands::{self, CommandSender, CommandsReceiver};
use crate::prelude::Resource;
use crate::resources::{Immutable, Resources};
use crate::schedule::{ConfigureObjects, ScheduleConfigure, ScheduleLabel};
use crate::system::System;

pub use access::SystemLock;
use gateway::{SystemDraft, SystemEntry};
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
    pub const fn as_unsafe_world_state(&mut self) -> &UnsafeWorldState {
        // Safety: by being the olny owner `&mut self`, this is allowed
        unsafe { &*(self as *const WorldState as *const UnsafeWorldState) }
    }

    pub fn as_unsafe_mut(&mut self) -> &UnsafeMutState {
        unsafe { self.as_unsafe_world_state().as_unsafe_mut() }
    }

    #[inline]
    pub fn take_resource<R: Resource>(&mut self) -> Option<R> {
        self.resources.try_take()
    }

    pub fn wrap(self) -> Arc<UnsafeWorldState> {
        let state = Arc::new(UnsafeWorldState::new(self));
        {
            let weak = WeakState(Arc::downgrade(&state));
            // Safety: we are the only owner
            unsafe {
                state.as_unsafe_mut().insert_resource(weak);
            }
        }

        state
    }
}

pub struct WeakState(std::sync::Weak<UnsafeWorldState>);
impl Resource for WeakState {
    type Mutability = Immutable;
}

impl WeakState {
    pub fn upgrade(&self) -> Option<Arc<UnsafeWorldState>> {
        self.0.upgrade()
    }
}

pub use unsafe_world_state::{UnsafeMutState, UnsafeWorldState};

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
        pub const fn new(world_state: WorldState) -> Self {
            Self(UnsafeCell::new(world_state))
        }

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
        #[allow(clippy::mut_from_ref)] // allow this since its unsafe
        pub unsafe fn get_resource_mut<R: Resource>(&self) -> Option<&mut R> {
            let resources = &mut unsafe { &mut *self.0.get() }.resources;
            let handle_ref = resources.handle_ref_mut()?;
            Some(unsafe { handle_ref.get_mut() })
        }

        pub fn commands(&self) -> CommandSender {
            unsafe { &*self.0.get() }.commands_sx.clone()
        }

        pub const fn as_world_state(&mut self) -> &mut WorldState {
            self.0.get_mut()
        }

        /// # Safety
        /// The caller must ensure it is valid to take / insert resources
        pub const unsafe fn as_unsafe_mut(&self) -> &UnsafeMutState {
            unsafe { &*(self as *const UnsafeWorldState as *const UnsafeMutState) }
        }
    }

    #[repr(transparent)]
    pub struct UnsafeMutState(UnsafeWorldState);

    impl UnsafeMutState {
        /// # Safety
        /// creating a UnsafeMutState already allows to take resources
        pub unsafe fn take_resource<R: Resource>(&self) -> Option<R> {
            unsafe { &mut *self.0.0.get() }.resources.try_take()
        }

        pub fn try_take_resource<R: Resource>(&mut self) -> Option<R> {
            self.0.0.get_mut().resources.try_take()
        }

        pub fn as_read(&self) -> &UnsafeWorldState {
            &self.0
        }

        /// # Safety
        /// creating a UnsafeMutState already allows to insert resources
        pub unsafe fn insert_resource<R: Resource>(&self, resource: R) {
            unsafe { &mut *self.0.0.get() }.resources.insert(resource);
        }

        /// # Safety
        /// Caller must ensure it is allowed to insert / remove resources
        pub unsafe fn borrow_world_mut<'w>(
            &'w self,
            locks: &'w mut crate::world::SystemLocks,
        ) -> crate::world::gateway::WorldMut<'w> {
            let state = unsafe { &mut *self.0.0.get() };
            crate::world::gateway::WorldMut::new(state, locks)
        }
    }
}

#[derive(Default)]
pub struct SystemLocks {
    rw: access::WorldLocks,
    pub systems_rw: meta::SystemsRw,
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

    pub fn is_all_free(&self) -> bool {
        self.rw.is_clean()
    }
}

pub struct WorldCenter {
    pub(crate) commands_rx: CommandsReceiver,
    pub system_locks: SystemLocks,
}

impl WorldCenter {
    pub fn tick_commands(&mut self, state: &mut WorldState) {
        for command in self.commands_rx.recv() {
            command.apply(state);
        }
    }

    pub fn register_system_rw(&mut self, system: &impl System) -> SystemId {
        let mut rw = SystemLock::default();
        system.init(&mut rw);

        self.system_locks.systems_rw.add(rw)
    }

    pub fn register_system<S: System>(&mut self, system: S) -> SystemEntry<S> {
        let id = self.register_system_rw(&system);
        SystemEntry::new(id, system)
    }

    pub fn register_draft<S: System>(&mut self, draft: SystemDraft<S>) -> SystemEntry<S> {
        let id = self.system_locks.systems_rw.add(draft.locks);
        SystemEntry::new(id, draft.system)
    }
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
    pub fn register_system_ref(&mut self, system: &impl System) -> SystemId {
        self.center.register_system_rw(system)
    }

    pub fn register_system<S: System>(&mut self, system: S) -> SystemEntry<S> {
        let id = self.center.register_system_rw(&system);
        SystemEntry::new(id, system)
    }

    pub fn into_parts(self) -> (WorldState, WorldCenter) {
        (self.state, self.center)
    }

    pub fn get<P: crate::param::Param>(&mut self) -> P::AsRef<'_> {
        self.get_and_center::<P>().0
    }

    pub fn get_and_center<P: crate::param::Param>(&mut self) -> (P::AsRef<'_>, &mut WorldCenter) {
        match self.try_get_and_center::<P>() {
            Some((p, c)) => (p, c),
            None => panic!("Could not get param `{}`", std::any::type_name::<P>()),
        }
    }

    pub fn try_get_and_center<P: crate::param::Param>(
        &mut self,
    ) -> Option<(P::AsRef<'_>, &mut WorldCenter)> {
        let mut system_locks = SystemLock::default();
        P::init(&mut system_locks);

        if !self.center.system_locks.can_lock_rw(&system_locks) {
            return None;
        }

        // Safety: Already checked with locks
        let param = unsafe { P::get_ref(self.state.as_unsafe_mut()) }.unwrap();
        Some((param, &mut self.center))
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

    fn add_systems<Sch, T, Marker>(mut self, schedule: Sch, into_cfg: T) -> Self
    where
        Sch: ScheduleLabel + ScheduleConfigure<T, Marker>,
    {
        schedule.add(self.world_mut(), into_cfg);

        self
    }

    fn add_objs<O, Marker>(mut self, label: impl ConfigureObjects<O, Marker>, objs: O) -> Self {
        label.add_objs(self.world_mut(), objs);
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
