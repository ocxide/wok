mod system_lock_runtime;

use std::marker::PhantomData;

use futures::{FutureExt, future::Either};
use lump_core::{
    async_executor::AsyncExecutor,
    runtime::RuntimeAddon,
    world::{
        SystemId, SystemLocks, UnsafeWorldState, WorldState,
        gateway::{SystemReleaseRx, SystemReleaser, WorldMut},
    },
};

use crate::{
    remote_gateway::{LockingGateway, RemoteGatewayRuntime},
    setup::AsyncExecutorabel,
};

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

pub struct RuntimeBuilder<Addon: RuntimeAddon> {
    addon: Addon,
    foreign_rt: RemoteGatewayRuntime,
    release_recv: SystemReleaseRx,
    releaser: SystemReleaser,
}

impl<Addon: RuntimeAddon> RuntimeBuilder<Addon> {
    pub fn new(state: &mut WorldState, addon: Addon) -> (Self, LockingGateway) {
        let (remote_gateway_rt, (remote_gateway, release_recv)) =
            RemoteGatewayRuntime::create(state);

        let this = Self {
            foreign_rt: remote_gateway_rt,
            addon,
            release_recv,
            releaser: remote_gateway.releaser.clone(),
        };

        (this, remote_gateway)
    }

    pub fn build<'a>(
        self,
        state: &'a UnsafeWorldState,
        locks: &'a mut SystemLocks,
    ) -> Runtime<'a, Addon> {
        Runtime {
            state,
            locks,
            addon: self.addon,
            foreign_rt: self.foreign_rt,
            release_recv: self.release_recv,
            releaser: Some(self.releaser),
        }
    }
}

pub struct Runtime<'w, Addon: RuntimeAddon> {
    state: &'w UnsafeWorldState,
    locks: &'w mut SystemLocks,
    addon: Addon,
    foreign_rt: RemoteGatewayRuntime,
    release_recv: SystemReleaseRx,
    releaser: Option<SystemReleaser>,
}

impl<'w, Addon: RuntimeAddon> Runtime<'w, Addon> {
    pub async fn run(&mut self, async_executor: &impl AsyncExecutor) {
        let mut foreign_rt_open = true;
        let mut release_recv_open = true;
        let mut addon_open = true;

        loop {
            if !foreign_rt_open && !release_recv_open && !addon_open {
                break;
            }

            let foreign_fut = if foreign_rt_open {
                Either::Left(self.foreign_rt.tick())
            } else {
                Either::Right(futures::future::pending::<Option<()>>())
            };

            let release_fut = if release_recv_open {
                Either::Left(self.release_recv.recv())
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
                        if let Some(releaser) = self.releaser.as_ref() {
                            let mut remote = WorldMut::new(self.state, self.locks).with_remote(releaser);
                            self.foreign_rt.act(async_executor, &mut remote);
                        }
                    }
                    else {
                        foreign_rt_open = false;
                    }
                }

                addon_tick = addon_tick.fuse() => {
                    if let Some(()) = addon_tick {
                        if let Some(releaser) = self.releaser.as_ref() {
                            let mut remote = WorldMut::new(self.state, self.locks).with_remote(releaser);
                            self.addon.act(async_executor, &mut remote);
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
                        self.locks.release(system_id);
                    }
                    else {
                        release_recv_open = false;
                    }
                }
            };
        }
    }
}
