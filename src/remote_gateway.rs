use crate::prelude::Param;
use std::{collections::VecDeque, sync::Arc};

use futures::{FutureExt, channel::oneshot};
use wok_core::{
    prelude::{
        BorrowMutParam, BorrowTaskSystem, DynTaskSystem, ProtoTaskSystem, Res, Resource, SystemIn,
        SystemInput,
    },
    runtime::RuntimeAddon,
    world::{
        SystemId, UnsafeWorldState, WeakState,
        gateway::{
            ReleaseSystem, SystemEntryRef, SystemReleaseRx, SystemReleaser, WeakSystemReleaser,
        },
    },
};

#[derive(Clone)]
pub struct LockingGateway {
    locker: async_channel::Sender<LockRequest>,
    pub releaser: SystemReleaser,
}

impl LockingGateway {
    pub fn downgrade(&self) -> WeakLockingGateway {
        WeakLockingGateway {
            locker: self.locker.downgrade(),
            releaser: self.releaser.downgrade(),
        }
    }
}

#[derive(Clone, Resource)]
#[resource(usage = lib)]
pub struct WeakLockingGateway {
    locker: async_channel::WeakSender<LockRequest>,
    releaser: WeakSystemReleaser,
}

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

#[derive(Clone)]
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

impl<'w, S> SystemTaskPermit<'w, S> {
    pub fn run<'i>(self, input: SystemIn<'i, S>) -> impl Future<Output = S::Out> + Send + 'i
    where
        S: ProtoTaskSystem<Param: BorrowMutParam>,
    {
        // Safety: Already checked with locks
        let param = unsafe { S::Param::borrow_owned(self.0.state) };
        let fut = <S as ProtoTaskSystem>::run(self.0.system.clone(), param, input);
        let releaser = self.0.releaser;

        fut.then(move |out| async move {
            releaser.release().await;
            out
        })
    }
}

// Use DynSystem instead of impl TaskSystem to prevent weird compile errors
impl<'w, In: SystemInput + 'static, Out: Send + Sync + 'static>
    SystemTaskPermit<'w, DynTaskSystem<In, Out>>
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

pub(crate) struct RemoteGatewayRuntime {
    rx: async_channel::Receiver<LockRequest>,
    buf: VecDeque<LockRequest>,
}

impl RuntimeAddon for RemoteGatewayRuntime {
    type Rests = (LockingGateway, SystemReleaseRx);
    fn create(state: &mut wok_core::prelude::WorldState) -> (Self, Self::Rests) {
        let (releaser, release_rx) = SystemReleaser::new();
        let (tx, rx) = async_channel::bounded(5);

        let this = Self {
            rx,
            buf: VecDeque::new(),
        };
        let gateway = LockingGateway {
            locker: tx,
            releaser,
        };

        state.resources.insert(gateway.downgrade());

        (this, (gateway, release_rx))
    }

    async fn tick(&mut self) -> Option<()> {
        let req = self.rx.recv().await.ok()?;

        self.buf.push_back(req);
        Some(())
    }

    fn act(
        &mut self,
        _async_executor: &impl wok_core::async_executor::AsyncExecutor,
        state: &mut wok_core::world::gateway::RemoteWorldMut<'_>,
    ) {
        while let Some(req) = self.buf.iter().next() {
            let result = state.world_mut().locks.try_lock(req.system_id);
            if result.is_err() {
                let req = self.buf.pop_front().unwrap();
                self.buf.push_back(req);

                continue;
            }
            drop(result);

            let req = self.buf.pop_front().unwrap();
            if req.respond_to.send(()).is_err() {
                state.world_mut().release(req.system_id);
            }
        }
    }
}
