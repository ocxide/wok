use futures::{FutureExt, StreamExt, channel::mpsc};
use locking::ForeignSystemLockingRuntime;
use lump_core::world::{SystemId, WorldCenter};

pub use locking::SystemLocking;

pub struct Runtime {
    pub(crate) world_center: WorldCenter,
    foreign_rt: ForeignSystemLockingRuntime,
    release_recv: ReleaseRecv,
}

impl Runtime {
    pub fn new(world_center: WorldCenter) -> (Self, SystemLocking) {
        let (tx, rx) = mpsc::channel(5);
        let release_recv = ReleaseRecv(rx);

        let (foreign_rt, locking) = ForeignSystemLockingRuntime::new(tx);

        let this = Self {
            foreign_rt,
            world_center,
            release_recv,
        };
    }

    pub async fn run(mut self) {
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
            println!("WARNING: failed to release system {}", self.system_id);
        }
    }
}

mod locking {
    use std::collections::VecDeque;

    use futures::{
        FutureExt, SinkExt, StreamExt,
        channel::{mpsc, oneshot},
    };
    use lump_core::{
        prelude::{DynSystem, SystemIn, SystemInput, TaskSystem},
        world::{SystemId, SystemLocks, WorldState},
    };

    use super::ReleaseSystem;

    #[derive(Clone)]
    pub struct SystemLocking {
        locker: mpsc::Sender<LockRequest>,
        releaser: mpsc::Sender<SystemId>,
    }

    impl SystemLocking {
        pub fn with_state(self, world: &WorldState) -> SystemLocker<'_> {
            SystemLocker {
                locking: self,
                world,
            }
        }
    }

    #[derive(Clone)]
    pub struct SystemLocker<'w> {
        locking: SystemLocking,
        world: &'w WorldState,
    }

    impl<'w> SystemInput for SystemLocker<'w> {
        type Inner<'i> = SystemLocker<'i>;
        type Wrapped<'i> = SystemLocker<'i>;

        fn wrap(this: Self::Inner<'_>) -> Self::Wrapped<'_> {
            this
        }
    }

    pub struct LockRequest {
        respond_to: oneshot::Sender<()>,
        system_id: SystemId,
    }

    impl<'w> SystemLocker<'w> {
        pub async fn lock(mut self, system_id: SystemId) -> LockedSystemParams<'w> {
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

            let releaser = ReleaseSystem {
                system_id,
                sx: self.locking.releaser,
            };

            LockedSystemParams {
                world: self.world,
                releaser,
            }
        }
    }

    pub struct LockedSystemParams<'w> {
        world: &'w WorldState,
        releaser: ReleaseSystem,
    }

    impl LockedSystemParams<'_> {
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
    }

    pub struct ForeignSystemLockingRuntime {
        rx: mpsc::Receiver<LockRequest>,
        buf: VecDeque<LockRequest>,
    }

    impl ForeignSystemLockingRuntime {
        pub fn new(releaser: mpsc::Sender<SystemId>) -> (Self, SystemLocking) {
            let (tx, rx) = mpsc::channel(5);

            (
                ForeignSystemLockingRuntime {
                    rx,
                    buf: VecDeque::new(),
                },
                SystemLocking {
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
}
