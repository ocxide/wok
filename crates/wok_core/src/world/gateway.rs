pub use local::*;
pub use owned_local::*;
pub use remote::*;
pub use system_entry::*;

mod remote {
    use async_channel as mpsc;
    use futures::FutureExt;

    use crate::{
        prelude::Resource,
        system::{BorrowTaskSystem, SystemIn},
        world::SystemId,
    };

    use super::{SystemEntryRef, WorldBorrowMut};

    #[derive(Debug, Clone)]
    pub struct ReleaseSystem {
        system_id: SystemId,
        sx: SystemReleaser,
    }

    impl ReleaseSystem {
        pub async fn release(self) {
            if self.sx.0.send(self.system_id).await.is_err() {
                println!("WARNING: failed to release system {:?}", self.system_id);
            };
        }
    }

    #[derive(Debug, Clone)]
    pub struct SystemReleaser(mpsc::Sender<SystemId>);
    pub struct SystemReleaseRx(mpsc::Receiver<SystemId>);

    impl SystemReleaseRx {
        pub async fn recv(&mut self) -> Option<SystemId> {
            self.0.recv().await.ok()
        }
    }

    impl SystemReleaser {
        pub fn downgrade(&self) -> WeakSystemReleaser {
            WeakSystemReleaser(self.0.downgrade())
        }

        pub fn close(self) {
            self.0.close();
        }
    }

    #[derive(Clone, Resource)]
    #[resource(usage = core)]
    pub struct WeakSystemReleaser(mpsc::WeakSender<SystemId>);

    impl WeakSystemReleaser {
        pub fn upgrade(&self) -> Option<SystemReleaser> {
            self.0.upgrade().map(SystemReleaser)
        }
    }

    impl SystemReleaser {
        pub fn new() -> (Self, SystemReleaseRx) {
            let (sx, rx) = mpsc::bounded(10);
            (Self(sx), SystemReleaseRx(rx))
        }
    }

    impl ReleaseSystem {
        pub fn new(system_id: SystemId, sx: SystemReleaser) -> Self {
            Self { system_id, sx }
        }
    }

    pub struct RemoteWorldMut<'w> {
        pub(crate) world_mut: WorldBorrowMut<'w>,
        pub(crate) releaser: &'w SystemReleaser,
    }

    impl Drop for ReleaseSystem {
        fn drop(&mut self) {
            if self.sx.0.try_send(self.system_id).is_err() {
                println!("WARNING: failed to release system {:?}", self.system_id);
            }
        }
    }

    impl<'w> RemoteWorldMut<'w> {
        pub fn create_world_mut(&'w mut self) -> WorldBorrowMut<'w> {
            self.world_mut.reborrow()
        }

        pub fn world_mut(&mut self) -> &mut WorldBorrowMut<'w> {
            &mut self.world_mut
        }

        pub fn try_run<'i, S: BorrowTaskSystem>(
            &mut self,
            system: SystemEntryRef<'_, S>,
            input: SystemIn<'i, S>,
        ) -> Result<impl Future<Output = S::Out> + 'i + Send, SystemIn<'i, S>> {
            let result = self.world_mut.locks.try_lock(system.id);
            if result.is_err() {
                return Err(input);
            }

            let release = ReleaseSystem::new(system.id, SystemReleaser(self.releaser.0.clone()));
            // Safety: Already checked with locks
            let fut = unsafe { system.system.run(self.world_mut.state, input) };
            Ok(fut.map(|out| {
                drop(release);
                out
            }))
        }

        pub fn duplicate(&'w mut self) -> Self {
            Self {
                world_mut: self.world_mut.reborrow(),
                releaser: self.releaser,
            }
        }
    }
}

mod local {
    use crate::{
        param::BorrowMutParam,
        world::{SystemId, SystemLock, SystemLocks, UnsafeWorldState},
    };

    use super::{RemoteWorldMut, SystemReleaser};

    pub struct WorldBorrowMut<'w> {
        pub(crate) state: &'w UnsafeWorldState,
        pub locks: &'w mut SystemLocks,
    }

    impl<'w> WorldBorrowMut<'w> {
        pub const fn reborrow<'w2: 'w>(&'w2 mut self) -> WorldBorrowMut<'w2> {
            Self {
                state: self.state,
                locks: self.locks,
            }
        }

        pub const fn new(state: &'w UnsafeWorldState, locks: &'w mut SystemLocks) -> Self {
            Self { state, locks }
        }

        pub fn get<P: BorrowMutParam>(&self) -> Option<P::AsRef<'_>> {
            let mut system_locks = SystemLock::default();
            P::init(&mut system_locks);

            if !self.locks.can_lock_rw(&system_locks) {
                return None;
            }
            // Safety: Already checked with locks
            Some(unsafe { P::borrow(self.state) })
        }

        pub fn with_remote(self, releaser: &'w SystemReleaser) -> RemoteWorldMut<'w> {
            RemoteWorldMut {
                world_mut: self,
                releaser,
            }
        }

        pub fn get_dyn(&self, getter: &ParamGetter) -> Option<Box<dyn std::any::Any + Send>> {
            if !self.locks.can_lock_rw(&getter.lock) {
                return None;
            }

            // Safety: Already checked with locks
            Some(unsafe { (getter.getter)(self.state) })
        }
    }

    impl WorldBorrowMut<'_> {
        pub fn release(&mut self, system_id: SystemId) {
            self.locks.release(system_id);
        }
    }

    pub struct ParamGetter {
        pub lock: SystemLock,
        getter: unsafe fn(&UnsafeWorldState) -> Box<dyn std::any::Any + Send>,
    }

    impl ParamGetter {
        pub fn new<P: BorrowMutParam>() -> Self {
            let mut lock = SystemLock::default();
            P::init(&mut lock);

            Self {
                lock,
                getter: |state| Box::new(unsafe { P::borrow_owned(state) }),
            }
        }
    }
}

mod owned_local {
    use futures::FutureExt;

    use crate::{
        param::Param,
        system::{
            BlockingCaller, BlockingSystem, ProtoBlockingSystem, ProtoTaskSystem, SystemIn,
            TaskSystem,
        },
        world::{SystemId, SystemLock, WorldSystemLockError},
    };

    use super::{SystemEntryRef, WorldBorrowMut};

    pub struct WorldMut<'w> {
        pub(crate) state: &'w mut crate::world::WorldState,
        pub locks: &'w mut crate::world::SystemLocks,
    }

    impl<'w> WorldMut<'w> {
        pub const fn new(
            state: &'w mut crate::world::WorldState,
            locks: &'w mut crate::world::SystemLocks,
        ) -> Self {
            Self { state, locks }
        }

        pub const fn reborrow<'w2>(&'w2 mut self) -> WorldMut<'w2> {
            WorldMut {
                state: self.state,
                locks: self.locks,
            }
        }

        pub fn reserve<'a, S>(
            &'a mut self,
            system: SystemEntryRef<'a, S>,
        ) -> Result<SystemPermit<'a, S>, WorldSystemLockError> {
            self.locks.try_lock(system.id)?;

            Ok(SystemPermit {
                world: self.reborrow(),
                system,
            })
        }

        pub const fn as_borrow(&'w mut self) -> WorldBorrowMut<'w> {
            WorldBorrowMut::new(self.state.as_unsafe_world_state(), self.locks)
        }

        pub fn get<P: Param>(self) -> P::AsRef<'w> {
            let mut system_locks = SystemLock::default();
            P::init(&mut system_locks);
            self.locks.can_lock_rw(&system_locks);

            // Safety: Already checked with locks
            unsafe { P::get_ref(self.state.as_unsafe_mut()) }
        }
    }

    pub struct SystemPermit<'w, S> {
        world: WorldMut<'w>,
        system: SystemEntryRef<'w, S>,
    }

    impl<'w, S> SystemPermit<'w, S> {
        pub const fn local_tasks(self) -> OwnedLocalTasks<'w, S> {
            OwnedLocalTasks(self)
        }

        pub const fn local_blocking(self) -> OwnedLocalBlocking<'w, S> {
            OwnedLocalBlocking(self)
        }
    }

    pub struct OwnedLocalTasks<'w, S>(pub SystemPermit<'w, S>);

    impl<'w, S> OwnedLocalTasks<'w, S> {
        pub fn run_dyn<'i>(
            self,
            input: SystemIn<'i, S>,
        ) -> impl Future<Output = (SystemId, S::Out)> + 'i + Send + use<'i, S>
        where
            S: TaskSystem,
        {
            // Safety: Already checked with locks
            let fut = unsafe {
                self.0
                    .system
                    .system
                    .owned_run(self.0.world.state.as_unsafe_mut(), input)
            };
            let id = self.0.system.id;
            fut.map(move |out| (id, out))
        }

        pub fn run<'i>(
            self,
            input: SystemIn<'i, S>,
        ) -> impl Future<Output = (SystemId, S::Out)> + 'i + use<'i, S> + Send
        where
            S: ProtoTaskSystem,
        {
            let param =
                unsafe { <S::Param as Param>::get_owned(self.0.world.state.as_unsafe_mut()) };

            // Safety: Already checked with locks
            let fut = self.0.system.system.clone().run(param, input);
            let id = self.0.system.id;
            fut.map(move |out| (id, out))
        }
    }

    pub struct OwnedLocalBlocking<'w, S>(pub SystemPermit<'w, S>);

    impl<'w, S> OwnedLocalBlocking<'w, S> {
        pub fn create_caller(self) -> BlockingCaller<S::In, S::Out>
        where
            S: BlockingSystem,
        {
            // Safety: Already checked with locks
            unsafe {
                self.0
                    .system
                    .system
                    .create_caller(self.0.world.state.as_unsafe_mut())
            }
        }

        pub fn run_dyn<'i>(&mut self, input: SystemIn<'i, S>) -> S::Out
        where
            S: BlockingSystem,
        {
            // Safety: Already checked with locks
            unsafe {
                self.0
                    .system
                    .system
                    .run(self.0.world.state.as_unsafe_mut(), input)
            }
        }

        pub fn run<'i>(&mut self, input: SystemIn<'i, S>) -> S::Out
        where
            S: ProtoBlockingSystem,
        {
            // Safety: Already checked with locks
            let param = unsafe { <S::Param as Param>::get_ref(self.0.world.state.as_unsafe_mut()) };
            self.0.system.system.run(param, input)
        }
    }
}

mod system_entry {
    use crate::{
        system::{BorrowTaskSystem, DynTaskSystem, System},
        world::{SystemId, SystemLock},
    };

    pub type TaskSystemEntry<In, Out> = SystemEntry<DynTaskSystem<In, Out>>;

    pub struct SystemEntryRef<'s, S> {
        pub system: &'s S,
        pub id: SystemId,
    }

    impl<'s, S> SystemEntryRef<'s, S> {
        pub fn new(id: SystemId, system: &'s S) -> Self {
            Self { system, id }
        }
    }

    #[derive(Clone)]
    pub struct SystemEntry<S> {
        pub system: S,
        pub id: SystemId,
    }

    impl<S> SystemEntry<S> {
        pub fn new(id: SystemId, system: S) -> Self {
            Self { system, id }
        }

        pub fn into_taskbox(self) -> TaskSystemEntry<S::In, S::Out>
        where
            S: BorrowTaskSystem,
        {
            TaskSystemEntry::new(self.id, Box::new(self.system))
        }

        pub fn entry_ref(&self) -> SystemEntryRef<S> {
            SystemEntryRef {
                system: &self.system,
                id: self.id,
            }
        }
    }

    pub type TaskSystemDraft<In, Out> = SystemDraft<DynTaskSystem<In, Out>>;

    pub struct SystemDraft<S> {
        pub(crate) system: S,
        pub(crate) locks: SystemLock,
    }

    impl<S: System> SystemDraft<S> {
        pub fn new(system: S) -> Self {
            let mut locks = SystemLock::default();
            system.init(&mut locks);
            Self { system, locks }
        }

        pub fn into_taskbox(self) -> TaskSystemDraft<S::In, S::Out>
        where
            S: BorrowTaskSystem,
        {
            TaskSystemDraft {
                system: Box::new(self.system),
                locks: self.locks,
            }
        }
    }
}
