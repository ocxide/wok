use std::collections::VecDeque;

use futures::{
    FutureExt, SinkExt, StreamExt,
    channel::{mpsc, oneshot},
};
use lump_core::{
    prelude::{DynSystem, SystemIn, SystemInput, TaskSystem},
    system_locking::{ReleaseSystem, SystemReleaser},
    world::{SystemId, SystemLocks, WorldState},
};

#[derive(Clone)]
pub struct LockingGateway {
    locker: mpsc::Sender<LockRequest>,
    releaser: SystemReleaser,
}

impl LockingGateway {
    pub fn with_state(self, world: &WorldState) -> SystemReserver<'_> {
        SystemReserver {
            locking: self,
            world,
        }
    }
}

#[derive(Clone)]
pub struct SystemReserver<'w> {
    locking: LockingGateway,
    world: &'w WorldState,
}

impl<'w> SystemInput for SystemReserver<'w> {
    type Inner<'i> = SystemReserver<'i>;
    type Wrapped<'i> = SystemReserver<'i>;

    fn wrap(this: Self::Inner<'_>) -> Self::Wrapped<'_> {
        this
    }
}

pub struct LockRequest {
    respond_to: oneshot::Sender<()>,
    system_id: SystemId,
}

impl<'w> SystemReserver<'w> {
    pub async fn lock(mut self, system_id: SystemId) -> SystemPermit<'w> {
        let (sx, rx) = oneshot::channel();

        let req = LockRequest {
            respond_to: sx,
            system_id,
        };

        self.locking
            .locker
            .send(req)
            .await
            .expect("to be connected to the main world");

        rx.await.expect("to be connected to the main world");

        let releaser = ReleaseSystem::new(system_id, self.locking.releaser);

        SystemPermit {
            world: self.world,
            releaser,
        }
    }
}

pub struct SystemPermit<'w> {
    world: &'w WorldState,
    releaser: ReleaseSystem,
}

impl SystemPermit<'_> {
    pub fn run<'i, S: TaskSystem>(
        self,
        system: &S,
        input: SystemIn<'i, S>,
    ) -> impl Future<Output = S::Out> + Send + 'i {
        system.run(self.world, input).map(move |out| {
            drop(self.releaser);
            out
        })
    }

    pub fn run_task<'i, In: SystemInput + 'static, Out: Send + Sync + 'static>(
        self,
        system: &DynSystem<In, Out>,
        input: In::Inner<'i>,
    ) -> impl Future<Output = Out> + Send + 'i {
        system.run(self.world, input).map(move |out| {
            drop(self.releaser);
            out
        })
    }
}

pub struct LockingQueue {
    rx: mpsc::Receiver<LockRequest>,
    buf: VecDeque<LockRequest>,
}

impl LockingQueue {
    pub fn new(releaser: SystemReleaser) -> (Self, LockingGateway) {
        let (tx, rx) = mpsc::channel(5);

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
        let req = self.rx.next().await?;

        self.buf.push_back(req);
        Some(())
    }

    pub fn try_respond(&mut self, locks: &mut SystemLocks) {
        while let Some(req) = self.buf.iter().next() {
            if locks.try_lock(req.system_id).is_err() {
                let req = self.buf.pop_front().unwrap();
                self.buf.push_back(req);

                continue;
            }

            let req = self.buf.pop_front().unwrap();
            if req.respond_to.send(()).is_err() {
                locks.release(req.system_id);
            }
        }
    }

    pub fn release(&mut self, system_id: SystemId, locks: &mut SystemLocks) {
        locks.release(system_id);
    }
}
