use lump_core::world::{ConfigureWorld, World, WorldState};
pub use runtime::{AsyncRuntime, RuntimeConfig, SystemTaskLauncher};
use runtime::{Invokers, Runtime};

use crate::{
    events::{Event, Events},
    startup::Startup,
};

pub struct AppBuilder<C: RuntimeConfig> {
    world: World,
    pub(crate) invokers: Invokers<C>,
}

impl<C: RuntimeConfig> Default for AppBuilder<C> {
    fn default() -> Self {
        let mut world = World::default();

        Startup::init(&mut world.center);

        Self {
            world,
            invokers: Default::default(),
        }
    }
}

impl<C: RuntimeConfig> AppBuilder<C> {
    pub fn build_parts(self, rt: C::AsyncRuntime) -> (Runtime<C>, WorldState) {
        let (state, center) = self.world.into_parts();

        let rt = Runtime::<C>::new(center, self.invokers, rt);
        (rt, state)
    }
}

impl<C: RuntimeConfig> ConfigureWorld for AppBuilder<C> {
    fn world(&self) -> &World {
        &self.world
    }

    fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}
impl<C: RuntimeConfig> AppBuilder<C> {
    pub fn register_event<E: Event>(mut self) -> Self {
        Events::register::<C, E>(&mut self);
        self
    }
}

mod runtime {
    use std::task::{Context, Poll};

    use futures::{FutureExt, StreamExt, stream::FuturesUnordered};
    use lump_core::{
        resources::LocalResources,
        world::{SystemId, SystemLocks, WorldCenter, WorldState, WorldSystemLockError},
    };

    use crate::startup::Startup;

    pub trait AsyncRuntime {
        type JoinHandle<T: Send + 'static>: Future<Output = T> + Send + 'static;
        fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<Fut::Output>
        where
            Fut: std::future::Future<Output: Send> + Send + 'static;
    }

    pub trait RuntimeConfig: 'static {
        type AsyncRuntime: AsyncRuntime;

        fn into_parts(self) -> Self::AsyncRuntime;
    }

    type Invoker<C: RuntimeConfig> =
        fn(&mut SystemTaskLauncher<'_, C>, &mut LocalResources, &WorldState);

    type InvokePoller = fn(&mut Context<'_>, &mut LocalResources) -> Poll<Option<()>>;
    pub struct PollingInvoker<C: RuntimeConfig> {
        poller: InvokePoller,
        invoker: Invoker<C>,
    }

    pub struct Invokers<C: RuntimeConfig> {
        invokers: Vec<Invoker<C>>,
        poll_invokers: Vec<PollingInvoker<C>>,
    }

    impl<C: RuntimeConfig> Invokers<C> {
        #[inline]
        pub fn add(&mut self, invoker: Invoker<C>) {
            self.invokers.push(invoker);
        }

        pub fn add_polling(&mut self, poller: InvokePoller, invoker: Invoker<C>) {
            self.poll_invokers.push(PollingInvoker { poller, invoker });
        }
    }

    impl<C: RuntimeConfig> Default for Invokers<C> {
        fn default() -> Self {
            Self {
                invokers: Vec::new(),
                poll_invokers: Vec::new(),
            }
        }
    }

    pub struct Runtime<C: RuntimeConfig> {
        world: WorldCenter,
        invokers: Invokers<C>,
        rt: C::AsyncRuntime,
    }

    impl<C: RuntimeConfig> Runtime<C> {
        pub fn new(world: WorldCenter, invokers: Invokers<C>, rt: C::AsyncRuntime) -> Self {
            Self {
                world,
                invokers,
                rt,
            }
        }

        pub async fn invoke_startup(&mut self, state: &mut WorldState) {
            let invoker = Startup::create_invoker::<C>(&mut self.world, state, &self.rt);
            invoker.invoke().await
        }

        fn create_launcher<'a>(
            &'a mut self,
            futures: &'a mut SystemFutures<C>,
        ) -> SystemTaskLauncher<'a, C> {
            SystemTaskLauncher::<C> {
                rt: &self.rt,
                futures,
                locks: &mut self.world.system_locks,
            }
        }

        fn on_system_complete(
            &mut self,
            futures: &mut SystemFutures<C>,
            state: &WorldState,
            systemid: SystemId,
        ) {
            self.world.system_locks.release(systemid);

            let mut launcher = SystemTaskLauncher::<C> {
                rt: &self.rt,
                futures,
                locks: &mut self.world.system_locks,
            };

            self.invokers.invokers.iter().for_each(|invoker| {
                invoker(&mut launcher, &mut self.world.resources, state);
            });
        }

        fn on_invoker_poll(
            &mut self,
            futures: &mut SystemFutures<C>,
            state: &WorldState,
            invoker: Invoker<C>,
        ) {
            let mut launcher = SystemTaskLauncher::<C> {
                rt: &self.rt,
                futures,
                locks: &mut self.world.system_locks,
            };

            invoker(&mut launcher, &mut self.world.resources, state);
        }

        pub async fn run(mut self, state: &WorldState) {
            let mut futures = SystemFutures::<C>::new();

            loop {
                let mut polling_fut = std::future::poll_fn(|cx| {
                    for invoker in self.invokers.poll_invokers.iter() {
                        let poll = (invoker.poller)(cx, &mut self.world.resources);

                        match poll {
                            Poll::Pending => {}
                            Poll::Ready(None) => return Poll::Ready(None),
                            Poll::Ready(Some(_)) => return Poll::Ready(Some(invoker.invoker)),
                        }
                    }

                    std::task::Poll::Pending
                })
                .fuse();

                futures::select! {
                    systemid = futures.next() => {
                        let Some(systemid) = systemid else {
                            break;
                        };

                        self.on_system_complete(&mut futures, state, systemid);
                    }

                    invoker = polling_fut => {
                        if let Some(invoker) = invoker {
                            self.on_invoker_poll(&mut futures, state, invoker);
                        }
                    }
                }
            }
        }
    }

    type SystemFutures<C> = FuturesUnordered<
        <<C as RuntimeConfig>::AsyncRuntime as AsyncRuntime>::JoinHandle<SystemId>,
    >;

    pub struct SystemTaskLauncher<'c, C: RuntimeConfig> {
        rt: &'c C::AsyncRuntime,
        futures: &'c SystemFutures<C>,
        locks: &'c mut SystemLocks,
    }

    pub struct LockedSystemLauncher<'c, C: RuntimeConfig> {
        systemid: SystemId,
        rt: &'c C::AsyncRuntime,
        futures: &'c SystemFutures<C>,
    }

    impl<'c, C: RuntimeConfig> LockedSystemLauncher<'c, C> {
        pub fn spawn(&self, fut: impl Future<Output = ()> + Send + 'static) {
            let systemid = self.systemid;
            let spawn = self.rt.spawn(async move {
                let _ = fut.await;
                systemid
            });

            self.futures.push(spawn);
        }
    }

    impl<C: RuntimeConfig> SystemTaskLauncher<'_, C> {
        pub fn single(
            &mut self,
            systemid: SystemId,
        ) -> Result<LockedSystemLauncher<'_, C>, WorldSystemLockError> {
            self.locks.try_lock(systemid)?;

            Ok(LockedSystemLauncher {
                systemid,
                rt: &self.rt,
                futures: &self.futures,
            })
        }
    }
}
