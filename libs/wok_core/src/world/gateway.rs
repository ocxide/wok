pub use local::*;
pub use remote::*;
pub use system_entry::*;

mod remote {
    use async_channel as mpsc;
    use futures::FutureExt;

    use crate::{
        prelude::Resource,
        system::{SystemIn, TaskSystem},
        world::SystemId,
    };

    use super::{SystemEntryRef, WorldMut};

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
    }

    #[derive(Clone)]
    pub struct WeakSystemReleaser(mpsc::WeakSender<SystemId>);
    impl Resource for WeakSystemReleaser {}

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
        pub(crate) world_mut: WorldMut<'w>,
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
        pub fn create_world_mut(&'w mut self) -> WorldMut<'w> {
            self.world_mut.duplicate()
        }

        pub fn world_mut(&mut self) -> &mut WorldMut<'w> {
            &mut self.world_mut
        }

        pub fn try_run<'i, S: TaskSystem>(
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
                world_mut: self.world_mut.duplicate(),
                releaser: self.releaser,
            }
        }
    }
}

mod local {
    use futures::FutureExt;

    use crate::{
        param::Param,
        system::{ProtoSystem, SystemIn, SystemTask, TaskSystem},
        world::{SystemId, SystemLock, SystemLocks, UnsafeWorldState, WorldSystemLockError},
    };

    use super::{RemoteWorldMut, SystemEntryRef, SystemReleaser};

    pub struct WorldMut<'w> {
        pub(crate) state: &'w UnsafeWorldState,
        pub locks: &'w mut SystemLocks,
    }

    impl<'w> WorldMut<'w> {
        pub fn duplicate(&'w mut self) -> Self {
            Self {
                state: self.state,
                locks: self.locks,
            }
        }

        pub fn new(state: &'w UnsafeWorldState, locks: &'w mut SystemLocks) -> Self {
            Self { state, locks }
        }

        pub fn get<P: Param>(&self) -> Option<P::AsRef<'_>> {
            let mut system_locks = SystemLock::default();
            P::init(&mut system_locks);

            if !self.locks.can_lock_rw(&system_locks) {
                return None;
            }
            // Safety: Already checked with locks
            Some(unsafe { P::get_ref(self.state) })
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

        pub fn local_tasks(&'w mut self) -> LocalTasks<'w> {
            LocalTasks(WorldMut {
                state: self.state,
                locks: self.locks,
            })
        }
    }

    impl WorldMut<'_> {
        pub fn release(&mut self, system_id: SystemId) {
            self.locks.release(system_id);
        }
    }

    pub struct ParamGetter {
        pub lock: SystemLock,
        getter: unsafe fn(&UnsafeWorldState) -> Box<dyn std::any::Any + Send>,
    }

    impl ParamGetter {
        pub fn new<P: Param>() -> Self {
            let mut lock = SystemLock::default();
            P::init(&mut lock);

            Self {
                lock,
                getter: |state| Box::new(unsafe { P::get(state) }),
            }
        }
    }

    pub struct LocalTasks<'w>(pub WorldMut<'w>);

    impl<'w> LocalTasks<'w> {
        pub fn run_dyn<'i, S>(
            &mut self,
            system: SystemEntryRef<'_, S>,
            input: SystemIn<'i, S>,
        ) -> Result<impl Future<Output = (SystemId, S::Out)> + 'i + Send, SystemIn<'i, S>>
        where
            S: TaskSystem,
        {
            let result = self.0.locks.try_lock(system.id);
            if result.is_err() {
                return Err(input);
            }

            // Safety: Already checked with locks
            let fut = unsafe { system.system.run(self.0.state, input) };
            let id = system.id;
            Ok(async move {
                let out = fut.await;
                (id, out)
            })
        }

        pub fn create_task<S>(
            &mut self,
            system: SystemEntryRef<'_, S>,
        ) -> Result<SystemTask<S::In, S::Out>, WorldSystemLockError>
        where
            S: TaskSystem,
        {
            self.0.locks.try_lock(system.id)?;

            // Safety: Already checked with locks
            Ok(unsafe { system.system.create_task(self.0.state) })
        }

        pub fn run<'i, S>(
            &mut self,
            system: SystemEntryRef<'_, S>,
            input: SystemIn<'i, S>,
        ) -> Result<impl Future<Output = (SystemId, S::Out)> + 'i + Send, SystemIn<'i, S>>
        where
            S: ProtoSystem,
        {
            if self.0.locks.try_lock(system.id).is_err() {
                return Err(input);
            }

            // Safety: Already checked with locks
            let param = unsafe { S::Param::get(self.0.state) };

            let fut = <S as ProtoSystem>::run(system.system.clone(), param, input);
            let id = system.id;
            Ok(fut.map(move |out| (id, out)))
        }
    }
}

mod system_entry {
    use crate::{
        system::{DynSystem, System, TaskSystem},
        world::{SystemId, SystemLock},
    };

    pub type TaskSystemEntry<In, Out> = SystemEntry<DynSystem<In, Out>>;

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
            S: TaskSystem,
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

    pub type TaskSystemDraft<In, Out> = SystemDraft<DynSystem<In, Out>>;

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
            S: TaskSystem,
        {
            TaskSystemDraft {
                system: Box::new(self.system),
                locks: self.locks,
            }
        }
    }
}
