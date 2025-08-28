mod params_client;

pub use params_client::*;

mod locking {
    use std::collections::VecDeque;

    use futures::{
        SinkExt, StreamExt,
        channel::{mpsc, oneshot},
    };
    use lump_core::{
        prelude::{DynSystem, ScopedFut, SystemInput, TaskSystem},
        world::{SystemId, SystemLocks, WorldState},
    };

    pub struct SystemLocking {
        locker: mpsc::Sender<LockRequest>,
    }

    impl SystemLocking {
        pub fn with_state(self, world: &WorldState) -> SystemLocker<'_> {
            SystemLocker {
                locker: self.locker,
                world,
            }
        }
    }

    #[derive(Clone)]
    pub struct SystemLocker<'w> {
        locker: mpsc::Sender<LockRequest>,
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

            self.locker
                .send(req)
                .await
                .expect("to be connected to the main world");

            rx.await.expect("to be connected to the main world");

            LockedSystemParams { world: self.world }
        }
    }

    pub struct LockedSystemParams<'w> {
        world: &'w WorldState,
    }

    impl LockedSystemParams<'_> {
        pub fn run<'i, In: SystemInput + 'static, Out: Send + Sync + 'static>(
            self,
            system: DynSystem<In, Out>,
            input: In::Inner<'i>,
        ) -> ScopedFut<'i, Out> {
            system.run(self.world, input)
        }
    }

    pub struct ForeignSystemLockingRuntime {
        rx: mpsc::Receiver<LockRequest>,
        buf: VecDeque<LockRequest>,
    }

    impl ForeignSystemLockingRuntime {
        pub fn new() -> (Self, SystemLocking) {
            let (tx, rx) = mpsc::channel(5);

            (
                ForeignSystemLockingRuntime {
                    rx,
                    buf: VecDeque::new(),
                },
                SystemLocking { locker: tx },
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
