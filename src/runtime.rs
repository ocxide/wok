mod system_lock_runtime;

use std::marker::PhantomData;

use futures::{FutureExt, StreamExt, channel::mpsc, future::Either};
use lump_core::{
    async_executor::AsyncExecutor,
    runtime::RuntimeAddon,
    system_locking::{StateLocker, SystemReleaser},
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
    pub fn with_addons<Addon2: RuntimeAddon>(self) -> RuntimeCfg<AR, Addon2> {
        RuntimeCfg {
            async_runtime: self.async_runtime,
            _addon_marker: PhantomData,
        }
    }

    /// Define the async runtime
    pub fn with_async<AR2: AsyncExecutorabel>(
        self,
        _: AR2,
    ) -> RuntimeCfg<AR2::AsyncRuntime, Addon> {
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

pub struct Runtime<'w, Addon: RuntimeAddon> {
    world_center: &'w mut WorldCenter,
    state: &'w WorldState,
    addon: Addon,
    foreign_rt: LockingQueue,
    release_recv: ReleaseRecv,
    releaser: Option<SystemReleaser>,
}

impl<'w, Addon: RuntimeAddon> Runtime<'w, Addon> {
    pub fn new(
        world_center: &'w mut WorldCenter,
        state: &'w WorldState,
        addon: Addon,
    ) -> (Self, LockingGateway) {
        let (tx, rx) = SystemReleaser::new();
        let release_recv = ReleaseRecv(rx);

        let (foreign_rt, locking) = LockingQueue::new(tx.clone());

        let this = Self {
            foreign_rt,
            world_center,
            state,
            addon,
            release_recv,
            releaser: Some(tx),
        };

        (this, locking)
    }

    pub async fn run(mut self, async_executor: &impl AsyncExecutor) {
        let mut foreign_rt_open = true;
        let mut release_recv_open = true;
        let mut addon_open = true;

        loop {
            if !foreign_rt_open && !release_recv_open && !addon_open {
                break;
            }

            let foreign_fut = if foreign_rt_open {
                Either::Left(self.foreign_rt.poll())
            } else {
                Either::Right(futures::future::pending::<Option<()>>())
            };

            let release_fut = if release_recv_open {
                Either::Left(self.release_recv.0.next())
            } else {
                Either::Right(futures::future::pending::<Option<SystemId>>())
            };

            let addon_tick = if addon_open {
                Either::Left(self.addon.tick())
            } else {
                Either::Right(futures::future::pending())
            };

            futures::select! {
                // Check for new requests of system locking
                next = foreign_fut.fuse() => {
                    if let Some(()) = next {
                        self.foreign_rt.try_respond(&mut self.world_center.system_locks);
                    }
                    else {
                        foreign_rt_open = false;
                    }
                }

                addon_tick = addon_tick.fuse() => {
                    if let Some(()) = addon_tick {
                        if let Some(releaser) = self.releaser.as_ref() {
                            self.addon.act(async_executor, &mut StateLocker::new(self.state, &mut self.world_center.system_locks, releaser));
                        }
                    }
                    else {
                        addon_open = false;
                        self.releaser = None;
                    }
                }

                // Release system locks
                system_id = release_fut.fuse() => {
                    if let Some(system_id) = system_id {
                        self.foreign_rt.release(system_id, &mut self.world_center.system_locks);
                    }
                    else {
                        release_recv_open = false;
                    }
                }
            };
        }
    }
}

struct ReleaseRecv(mpsc::Receiver<SystemId>);
