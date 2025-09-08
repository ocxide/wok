use futures::{FutureExt, SinkExt, channel::mpsc};

use crate::{
    param::Param,
    system::{DynSystem, SystemInput},
    world::{SystemId, SystemLock, SystemLocks, WorldState},
};

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

pub struct StateLocker<'w> {
    pub state: &'w WorldState,
    pub locks: &'w mut SystemLocks,
    releaser: &'w SystemReleaser,
}

impl Drop for ReleaseSystem {
    fn drop(&mut self) {
        if self.sx.0.try_send(self.system_id).is_err() {
            println!("WARNING: failed to release system {:?}", self.system_id);
        }
    }
}

impl<'w> StateLocker<'w> {
    pub fn new(
        state: &'w WorldState,
        locks: &'w mut SystemLocks,
        releaser: &'w SystemReleaser,
    ) -> Self {
        Self {
            state,
            locks,
            releaser,
        }
    }

    pub fn run_task<'i, In: SystemInput + 'static, Out: Send + Sync + 'static>(
        &mut self,
        systemid: SystemId,
        system: &DynSystem<In, Out>,
        input: In::Inner<'i>,
    ) -> Result<impl Future<Output = Out> + 'i + Send, In::Inner<'i>> {
        let result = self.locks.try_lock(systemid);
        if result.is_err() {
            return Err(input);
        }

        let release = ReleaseSystem::new(systemid, SystemReleaser(self.releaser.0.clone()));

        let fut = system.run(self.state, input);
        Ok(fut.map(|out| {
            drop(release);
            out
        }))
    }

    pub fn get<P: Param>(&mut self) -> Option<P::AsRef<'w>> {
        let mut system_locks = SystemLock::default();
        P::init(&mut system_locks);

        if !self.locks.can_lock_rw(&system_locks) {
            return None;
        }
        Some(P::get_ref(self.state))
    }
}
