// Request resources at runtime

use std::{any::Any, collections::VecDeque};

use futures::{
    SinkExt, StreamExt,
    channel::{mpsc, oneshot},
};
use wok_core::{
    async_executor::AsyncExecutor,
    prelude::{BorrowMutParam, Resource},
    runtime::RuntimeAddon,
    world::gateway::{ParamGetter, RemoteWorldMut, WorldBorrowMut},
    world::{SystemLock, WorldState},
};

struct ParamsResponse {
    params: Box<dyn Any + Send>,
    key: ForeignParamsKey,
}

struct LockParamsRequest {
    getter: ParamGetter,
    respond_to: oneshot::Sender<ParamsResponse>,
}

#[derive(Clone, Resource)]
#[resource(usage = lib)]
pub struct ParamsClient {
    requester: mpsc::Sender<LockParamsRequest>,
    close_sender: mpsc::Sender<ForeignParamsKey>,
}

pub struct ParamGuard<P: BorrowMutParam> {
    params: P::Owned,
    key: ForeignParamsKey,
    close_sender: mpsc::Sender<ForeignParamsKey>,
}

impl<P: BorrowMutParam> Drop for ParamGuard<P> {
    fn drop(&mut self) {
        if self.close_sender.try_send(self.key).is_err() {
            eprintln!("WARNING: failed to close foreign param");
        }
    }
}

impl<P: BorrowMutParam> ParamGuard<P> {
    pub fn get(&mut self) -> P::AsRef<'_> {
        P::from_owned(&mut self.params)
    }
}

impl ParamsClient {
    pub async fn get<P: BorrowMutParam>(&mut self) -> ParamGuard<P> {
        let mut lock = SystemLock::default();
        P::init(&mut lock);

        let (sx, rx) = oneshot::channel();

        let locker = LockParamsRequest {
            getter: ParamGetter::new::<P>(),
            respond_to: sx,
        };

        self.requester
            .send(locker)
            .await
            .expect("to be connected to the main world");
        let response = rx.await.expect("to be connected to the main world");

        let params = *response.params.downcast().expect("to be the right type");

        ParamGuard {
            params,
            key: response.key,
            close_sender: self.close_sender.clone(),
        }
    }
}

struct UnorderedQueue<T> {
    values: Vec<Option<T>>,
}

impl<T> Default for UnorderedQueue<T> {
    fn default() -> Self {
        Self { values: Vec::new() }
    }
}

impl<T> UnorderedQueue<T> {
    /// Returns the index where the value was inserted
    pub fn add(&mut self, value: T) -> usize {
        if let Some((i, element)) = self
            .values
            .iter_mut()
            .enumerate()
            .find(|(_, v)| v.is_none())
        {
            *element = Some(value);
            return i;
        }

        self.values.push(Some(value));
        self.values.len() - 1
    }

    pub fn remove_at(&mut self, index: usize) -> Option<T> {
        self.values.get_mut(index)?.take()
    }
}

#[derive(Clone, Copy, Hash, Eq, PartialEq)]
pub(crate) struct ForeignParamsKey(usize);

pub struct WokParamsClientRuntime {
    requests: mpsc::Receiver<LockParamsRequest>,
    closes: mpsc::Receiver<ForeignParamsKey>,
    pending_close: Option<ForeignParamsKey>,
    buf: VecDeque<LockParamsRequest>,
    foreign_locks: UnorderedQueue<SystemLock>,
}

impl RuntimeAddon for WokParamsClientRuntime {
    type Rests = ();

    fn create(state: &mut WorldState) -> (Self, Self::Rests) {
        let (this, client) = WokParamsClientRuntime::new();
        state.resources.insert(client);

        (this, ())
    }

    async fn tick(&mut self) -> Option<()> {
        futures::select! {
            req = self.requests.next() => {
                let req = req?;
                self.buf.push_back(req);
            }
            key = self.closes.next() => {
                let key = key?;
                self.pending_close = Some(key);
            }
        }

        Some(())
    }

    fn act(&mut self, _async_executor: &impl AsyncExecutor, state: &mut RemoteWorldMut<'_>) {
        self.try_lend(state);

        if let Some(key) = self.pending_close.take() {
            self.release(key, state.world_mut());
        }
    }
}

impl WokParamsClientRuntime {
    pub fn new() -> (Self, ParamsClient) {
        let (requester, requests) = mpsc::channel(1);
        let (close_sender, close_receiver) = mpsc::channel(1);

        (
            WokParamsClientRuntime {
                requests,
                buf: VecDeque::new(),
                closes: close_receiver,
                pending_close: None,
                foreign_locks: UnorderedQueue::default(),
            },
            ParamsClient {
                requester,
                close_sender,
            },
        )
    }

    fn release(&mut self, key: ForeignParamsKey, state: &mut WorldBorrowMut<'_>) {
        if let Some(lock) = self.foreign_locks.remove_at(key.0) {
            state.locks.release_rw(&lock);
        }
    }

    fn try_lend(&mut self, state: &mut RemoteWorldMut<'_>) {
        while let Some(locking) = self.buf.pop_front() {
            let params = state.world_mut().get_dyn(&locking.getter);

            let params = match params {
                Some(params) => params,
                None => {
                    self.buf.push_back(locking);

                    continue;
                }
            };


            let key = self.foreign_locks.add(locking.getter.lock);
            if let Err(response) = locking.respond_to.send(ParamsResponse {
                params,
                key: ForeignParamsKey(key),
            }) {
                eprintln!("WARNING: failed to respond to foreign param request");
                self.release(response.key, state.world_mut());
            }
        }
    }
}
