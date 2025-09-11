use crate::prelude::Param;
use std::{collections::VecDeque, sync::Arc};

use futures::channel::oneshot;
use lump_core::{
    prelude::{DynSystem, Res, Resource, SystemIn, SystemInput, TaskSystem},
    system_locking::{ReleaseSystem, SystemEntryRef, SystemReleaser, WeakSystemReleaser, WorldMut},
    world::{SystemId, UnsafeWorldState, WeakState},
};

#[derive(Clone)]
pub struct LockingGateway {
    locker: async_channel::Sender<LockRequest>,
    releaser: SystemReleaser,
}

impl LockingGateway {
    pub fn downgrade(&self) -> WeakLockingGateway {
        WeakLockingGateway {
            locker: self.locker.downgrade(),
            releaser: self.releaser.downgrade(),
        }
    }
}

#[derive(Clone)]
pub struct WeakLockingGateway {
    locker: async_channel::WeakSender<LockRequest>,
    releaser: WeakSystemReleaser,
}
impl Resource for WeakLockingGateway {}

impl WeakLockingGateway {
    pub fn upgrade(&self) -> Option<LockingGateway> {
        let locker = self.locker.upgrade()?;
        let releaser = self.releaser.upgrade()?;

        Some(LockingGateway { locker, releaser })
    }
}

pub struct LockRequest {
    respond_to: oneshot::Sender<()>,
    system_id: SystemId,
}

pub struct RemoteWorldPorts {
    state: Arc<UnsafeWorldState>,
    locking: LockingGateway,
}

impl RemoteWorldPorts {
    pub fn reserver(&self) -> RemoteSystemReserver<'_> {
        RemoteSystemReserver {
            state: &self.state,
            gateway: &self.locking,
        }
    }
}

#[derive(Param)]
#[param(usage = lib)]
pub struct RemoteWorldRef<'w> {
    state: Res<'w, WeakState>,
    gateway: Res<'w, WeakLockingGateway>,
}

impl<'w> RemoteWorldRef<'w> {
    pub fn upgrade(self) -> Option<RemoteWorldPorts> {
        let state = self.state.upgrade()?;
        let gateway = self.gateway.upgrade()?;

        Some(RemoteWorldPorts {
            state,
            locking: gateway,
        })
    }
}

pub struct RemoteSystemReserver<'w> {
    state: &'w UnsafeWorldState,
    gateway: &'w LockingGateway,
}

impl<'w> RemoteSystemReserver<'w> {
    pub async fn reserve<S>(&self, system: SystemEntryRef<'w, S>) -> SystemPermit<'w, S> {
        let (tx, rx) = oneshot::channel();

        let request = LockRequest {
            respond_to: tx,
            system_id: system.id,
        };
        self.gateway
            .locker
            .send(request)
            .await
            .expect("to be connected");

        rx.await.expect("to receive confirmation");

        SystemPermit {
            state: self.state,
            system: system.system,
            releaser: ReleaseSystem::new(system.id, self.gateway.releaser.clone()),
        }
    }
}

pub struct SystemPermit<'w, S> {
    state: &'w UnsafeWorldState,
    system: &'w S,
    releaser: ReleaseSystem,
}

impl<'w, S> SystemPermit<'w, S> {
    pub const fn task(self) -> SystemTaskPermit<'w, S> {
        SystemTaskPermit(self)
    }
}

pub struct SystemTaskPermit<'w, S>(SystemPermit<'w, S>);

// Use DynSystem instead of impl TaskSystem to prevent weird compile errors
impl<'w, In: SystemInput + 'static, Out: Send + Sync + 'static>
    SystemTaskPermit<'w, DynSystem<In, Out>>
{
    pub fn run_dyn<'i>(self, input: In::Inner<'i>) -> impl Future<Output = Out> + Send + 'i {
        // Safety: Already checked with locks
        let fut = unsafe { self.0.system.run(self.0.state, input) };
        let releaser = self.0.releaser;

        async move {
            let out = fut.await;
            releaser.release().await;

            out
        }
    }
}

pub struct LockingQueue {
    rx: async_channel::Receiver<LockRequest>,
    buf: VecDeque<LockRequest>,
}

impl LockingQueue {
    pub fn new(releaser: SystemReleaser) -> (Self, LockingGateway) {
        let (tx, rx) = async_channel::bounded(5);

        (
            LockingQueue {
                rx,
                buf: VecDeque::new(),
            },
            LockingGateway {
                locker: tx,
                releaser,
            },
        )
    }

    pub async fn poll(&mut self) -> Option<()> {
        let req = self.rx.recv().await.ok()?;

        self.buf.push_back(req);
        Some(())
    }

    pub fn try_respond(&mut self, world: &mut WorldMut<'_>) {
        while let Some(req) = self.buf.iter().next() {
            let result = world.locks.try_lock(req.system_id);
            if result.is_err() {
                let req = self.buf.pop_front().unwrap();
                self.buf.push_back(req);

                continue;
            }
            drop(result);

            let req = self.buf.pop_front().unwrap();
            if req.respond_to.send(()).is_err() {
                world.release(req.system_id);
            }
        }
    }
}
