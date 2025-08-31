mod system_lock_runtime;

use std::marker::PhantomData;

use futures::{FutureExt, StreamExt, channel::mpsc};
use lump_core::{
    runtime::RuntimeAddon,
    world::{SystemId, WorldCenter, WorldState},
};
use system_lock_runtime::LockingQueue;

pub use system_lock_runtime::{LockingGateway, SystemPermit, SystemReserver};

use crate::setup::AsyncExecutorabel;

pub struct RuntimeCfg<AR = (), Addon = ()> {
    pub async_runtime: AR,
    _addon_marker: PhantomData<Addon>,
}

impl<AR, Addon> RuntimeCfg<AR, Addon> {
    pub fn use_addons<Addon2: RuntimeAddon>(self) -> RuntimeCfg<AR, Addon2> {
        RuntimeCfg {
            async_runtime: self.async_runtime,
            _addon_marker: PhantomData,
        }
    }

    /// Define the async runtime
    pub fn use_async<AR2: AsyncExecutorabel>(self, _: AR2) -> RuntimeCfg<AR2::AsyncRuntime, Addon> {
        RuntimeCfg {
            async_runtime: AR2::create(),
            _addon_marker: PhantomData,
        }
    }

    /// Set the async runtime
    pub fn with_async_rt<AR2: AsyncExecutorabel>(self, rt: AR2) -> RuntimeCfg<AR2, Addon> {
        RuntimeCfg {
            async_runtime: rt,
            _addon_marker: PhantomData,
        }
    }
}

impl Default for RuntimeCfg {
    fn default() -> Self {
        RuntimeCfg {
            async_runtime: (),
            _addon_marker: PhantomData,
        }
    }
}

pub struct Runtime {
    pub(crate) world_center: WorldCenter,
    foreign_rt: LockingQueue,
    release_recv: ReleaseRecv,
}

impl Runtime {
    pub fn new(world_center: WorldCenter) -> (Self, LockingGateway) {
        let (tx, rx) = mpsc::channel(5);
        let release_recv = ReleaseRecv(rx);

        let (foreign_rt, locking) = LockingQueue::new(tx);

        let this = Self {
            foreign_rt,
            world_center,
            release_recv,
        };

        (this, locking)
    }

    pub async fn run(mut self, state: &WorldState, mut addon: impl RuntimeAddon) {
        loop {
            futures::select! {
                // Check for new requests of system locking
                next = self.foreign_rt.poll().fuse() => {
                    if let Some(()) = next {
                        self.foreign_rt.try_respond(&mut self.world_center.system_locks);
                    }
                    else {
                        break;
                    }
                }

                _ = addon.tick().fuse() => {
                    addon.act(state, &mut self.world_center.system_locks);
                }

                // Release system locks
                system_id = self.release_recv.0.next() => {
                    if let Some(system_id) = system_id {
                        self.foreign_rt.release(system_id, &mut self.world_center.system_locks);
                    }
                    else {
                        break;
                    }
                }
            }
        }
    }
}

struct ReleaseRecv(mpsc::Receiver<SystemId>);
struct ReleaseSystem {
    system_id: SystemId,
    sx: mpsc::Sender<SystemId>,
}

impl Drop for ReleaseSystem {
    fn drop(&mut self) {
        if self.sx.try_send(self.system_id).is_err() {
            println!("WARNING: failed to release system {:?}", self.system_id);
        }
    }
}
