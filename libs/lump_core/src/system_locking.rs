pub use local::*;
pub use remote::*;
pub use system_entry::*;

mod remote {
    use futures::{FutureExt, SinkExt, channel::mpsc};

    use crate::{
        system::{DynSystem, SystemInput},
        world::SystemId,
    };

    use super::{SystemEntryRef, WorldMut};

    #[derive(Debug, Clone)]
    pub struct ReleaseSystem {
        system_id: SystemId,
        sx: SystemReleaser,
    }

    impl ReleaseSystem {
        pub async fn release(mut self) {
            if self.sx.0.send(self.system_id).await.is_err() {
                println!("WARNING: failed to release system {:?}", self.system_id);
            };
        }
    }

    #[derive(Debug, Clone)]
    pub struct SystemReleaser(mpsc::Sender<SystemId>);

    impl SystemReleaser {
        pub fn new() -> (Self, mpsc::Receiver<SystemId>) {
            let (sx, rx) = mpsc::channel(0);
            (Self(sx), rx)
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
        pub fn world_mut(&'w mut self) -> WorldMut<'w> {
            self.world_mut.duplicate()
        }

        pub fn try_run<'i, In: SystemInput + 'static, Out: Send + Sync + 'static>(
            &mut self,
            system: SystemEntryRef<'_, DynSystem<In, Out>>,
            input: In::Inner<'i>,
        ) -> Result<impl Future<Output = Out> + 'i + Send, In::Inner<'i>> {
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
    }
}

mod local {
    use crate::{
        param::Param,
        system::{SystemIn, TaskSystem},
        world::{SystemId, SystemLock, SystemLocks, UnsafeWorldState},
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

        pub fn run_task<'i, S>(
            &mut self,
            system: SystemEntryRef<'_, S>,
            input: SystemIn<'i, S>,
        ) -> Result<impl Future<Output = (SystemId, S::Out)> + 'i + Send, SystemIn<'i, S>>
        where
            S: TaskSystem,
        {
            let result = self.locks.try_lock(system.id);
            if result.is_err() {
                return Err(input);
            }

            // Safety: Already checked with locks
            let fut = unsafe { system.system.run(self.state, input) };
            let id = system.id;
            Ok(async move {
                let out = fut.await;
                (id, out)
            })
        }

        pub fn remote(self, releaser: &'w SystemReleaser) -> RemoteWorldMut<'w> {
            RemoteWorldMut {
                world_mut: self,
                releaser,
            }
        }
    }

    impl WorldMut<'_> {
        pub fn release(&mut self, system_id: SystemId) {
            self.locks.release(system_id);
        }
    }
}

mod system_entry {
    use crate::{
        system::{DynSystem, SystemInput},
        world::SystemId,
    };

    pub struct TaskSystemEntry<In: SystemInput + 'static, Out: Send + Sync + 'static> {
        system: DynSystem<In, Out>,
        id: SystemId,
    }

    impl<In: SystemInput + 'static, Out: Send + Sync + 'static> TaskSystemEntry<In, Out> {
        pub fn entry_ref(&self) -> SystemEntryRef<DynSystem<In, Out>> {
            SystemEntryRef {
                system: &self.system,
                id: self.id,
            }
        }
    }

    pub struct SystemEntryRef<'s, S> {
        pub system: &'s S,
        pub id: SystemId,
    }

    impl<'s, S> SystemEntryRef<'s, S> {
        pub fn new(id: SystemId, system: &'s S) -> Self {
            Self { system, id }
        }
    }
}
