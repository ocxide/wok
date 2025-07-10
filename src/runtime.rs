use std::task::{Context, Poll};

use futures::{FutureExt, StreamExt, stream::FuturesUnordered};
use lump_core::{
    resources::LocalResources,
    world::{SystemId, SystemLocks, WorldCenter, WorldState, WorldSystemLockError},
};

use crate::{
    foreign::{ParamsLender, ParamsLenderPorts},
    startup::Startup,
};

pub trait AsyncRuntime {
    type JoinHandle<T: Send + 'static>: Future<Output = T> + Send + 'static;
    fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<Fut::Output>
    where
        Fut: std::future::Future<Output: Send> + Send + 'static;
}

pub trait RuntimeConfig: 'static {
    type AsyncRuntime: AsyncRuntime;
}

type Invoker<C> = fn(&mut SystemTaskLauncher<'_, C>, &mut LocalResources, &WorldState);

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

pub(crate) struct MainRuntime<C: RuntimeConfig> {
    pub(crate) world: WorldCenter,
    invokers: Invokers<C>,
}

impl<C: RuntimeConfig> MainRuntime<C> {
    pub async fn invoke_startup(&mut self, rt: &C::AsyncRuntime, state: &mut WorldState) {
        let invoker = Startup::create_invoker::<C>(&mut self.world, state, rt);
        invoker.invoke().await
    }

    fn on_system_complete(
        &mut self,
        rt: &C::AsyncRuntime,
        futures: &mut SystemFutures<C>,
        state: &WorldState,
        systemid: SystemId,
    ) {
        self.world.system_locks.release(systemid);

        let mut launcher = SystemTaskLauncher::<C> {
            rt,
            futures,
            locks: &mut self.world.system_locks,
        };

        self.invokers.invokers.iter().for_each(|invoker| {
            invoker(&mut launcher, &mut self.world.resources, state);
        });
    }

    fn on_invoker_poll(
        &mut self,
        rt: &C::AsyncRuntime,
        futures: &mut SystemFutures<C>,
        state: &WorldState,
        invoker: Invoker<C>,
    ) {
        let mut launcher = SystemTaskLauncher::<C> {
            rt,
            futures,
            locks: &mut self.world.system_locks,
        };

        invoker(&mut launcher, &mut self.world.resources, state);
    }
}

pub struct Runtime<C: RuntimeConfig> {
    pub(crate) main: MainRuntime<C>,
    pub(crate) lender: (ParamsLender, ParamsLenderPorts),
    rt: C::AsyncRuntime,
}

impl<C: RuntimeConfig> Runtime<C> {
    pub(crate) fn new(
        world: WorldCenter,
        invokers: Invokers<C>,
        lender: (ParamsLender, ParamsLenderPorts),
        rt: C::AsyncRuntime,
    ) -> Self {
        Self {
            main: MainRuntime { world, invokers },
            lender,
            rt,
        }
    }

    pub async fn invoke_startup(&mut self, state: &mut WorldState) {
        self.main.invoke_startup(&self.rt, state).await
    }

    pub async fn run(self, state: &WorldState) {
        let mut futures = SystemFutures::<C>::new();
        let Runtime {
            mut main,
            lender: (mut params_lender, mut lender_ports),
            rt,
        } = self;

        loop {
            let mut polling_fut = std::future::poll_fn(|cx| {
                for invoker in main.invokers.poll_invokers.iter() {
                    let poll = (invoker.poller)(cx, &mut main.world.resources);

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
                 params_key = lender_ports.close_sender.next() => {
                    let Some(params_key) = params_key else {
                        println!("Params ports closed");
                        break;
                    };

                    params_lender.release(params_key, &mut main.world.system_locks);
                }

                not_end = params_lender.tick().fuse() => {
                    if not_end.is_none() {
                        println!("Params lender closed");
                        break;
                    }

                    params_lender.try_respond_queue(&mut main.world.system_locks, state);
                }

                systemid = futures.next() => {
                    let Some(systemid) = systemid else {
                        println!("systems Futures closed");
                        break;
                    };

                    main.on_system_complete(&rt, &mut futures, state, systemid);
                }

                invoker = polling_fut => {
                    if let Some(invoker) = invoker {
                        main.on_invoker_poll(&rt, &mut futures, state, invoker);
                    }
                }
            }
        }
    }
}

type SystemFutures<C> = FuturesUnordered<SystemHandle<C>>;

pub type SystemHandle<C> =
    <<C as RuntimeConfig>::AsyncRuntime as AsyncRuntime>::JoinHandle<SystemId>;

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
            rt: self.rt,
            futures: self.futures,
        })
    }
}

pub mod tokio {
    use futures::FutureExt;
    use tokio::{runtime::Handle, task::JoinHandle};

    use crate::runtime::AsyncRuntime;

    pub struct TokioJoinHandle<T>(pub JoinHandle<T>);

    impl<T> Future for TokioJoinHandle<T> {
        type Output = T;
        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            self.0
                .poll_unpin(cx)
                .map(|poll| poll.expect("Tokio join handle failed"))
        }
    }

    impl AsyncRuntime for Handle {
        type JoinHandle<T: Send + 'static> = TokioJoinHandle<T>;

        fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<Fut::Output>
        where
            Fut: std::future::Future<Output: Send> + Send + 'static,
        {
            let handle = self.spawn(fut);
            TokioJoinHandle(handle)
        }
    }
}
