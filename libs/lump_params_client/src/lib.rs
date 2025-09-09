// Request resources at runtime

use std::{any::Any, collections::VecDeque};

use futures::{
    SinkExt, StreamExt,
    channel::{mpsc, oneshot},
};
use lump_core::{
    async_executor::AsyncExecutor,
    prelude::{Param, Resource},
    runtime::RuntimeAddon,
    system_locking::RemoteWorldMut,
    world::{SystemLock, SystemLocks, WorldState},
};

struct ParamsResponse {
    params: Box<dyn Any + Send>,
    key: ForeignParamsKey,
}

struct LockParamsRequest {
    param_getter: fn(&WorldState) -> Box<dyn Any + Send>,
    system_rw: SystemLock,
    respond_to: oneshot::Sender<ParamsResponse>,
}

#[derive(Clone)]
pub struct ParamsClient {
    requester: mpsc::Sender<LockParamsRequest>,
    close_sender: mpsc::Sender<ForeignParamsKey>,
}

impl Resource for ParamsClient {}

pub struct ParamGuard<P: Param> {
    params: P::Owned,
    key: ForeignParamsKey,
    close_sender: mpsc::Sender<ForeignParamsKey>,
}

impl<P: Param> Drop for ParamGuard<P> {
    fn drop(&mut self) {
        if self.close_sender.try_send(self.key).is_err() {
            eprintln!("WARNING: failed to close foreign param");
        }
    }
}

impl<P: Param> ParamGuard<P> {
    pub fn get(&self) -> P::AsRef<'_> {
        P::from_owned(&self.params)
    }
}

impl ParamsClient {
    pub async fn get<P: Param>(&mut self) -> ParamGuard<P> {
        let mut lock = SystemLock::default();
        P::init(&mut lock);

        let (sx, rx) = oneshot::channel();

        let locker = LockParamsRequest {
            param_getter: |state| Box::new(P::get(state)),
            system_rw: lock,
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

pub struct LumpParamsClientRuntime {
    requests: mpsc::Receiver<LockParamsRequest>,
    closes: mpsc::Receiver<ForeignParamsKey>,
    pending_close: Option<ForeignParamsKey>,
    buf: VecDeque<LockParamsRequest>,
    foreign_locks: UnorderedQueue<SystemLock>,
}

impl RuntimeAddon for LumpParamsClientRuntime {
    fn create(state: &mut WorldState) -> Self {
        let (this, client) = LumpParamsClientRuntime::new();

        state.resources.insert(client);

        this
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

    fn act(&mut self, _: &impl AsyncExecutor, state: &mut RemoteWorldMut<'_>) {
        self.try_lend(state);

        if let Some(key) = self.pending_close.take() {
            self.release(key, state.locks);
        }
    }
}

impl LumpParamsClientRuntime {
    pub fn new() -> (Self, ParamsClient) {
        let (requester, requests) = mpsc::channel(1);
        let (close_sender, close_receiver) = mpsc::channel(1);

        (
            LumpParamsClientRuntime {
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

    fn release(&mut self, key: ForeignParamsKey, locks: &mut SystemLocks) {
        if let Some(lock) = self.foreign_locks.remove_at(key.0) {
            locks.release_rw(&lock);
        }
    }

    fn try_lend(&mut self, state: &mut RemoteWorldMut<'_>) {
        while let Some(locking) = self.buf.iter().next() {
            if state.locks.try_lock_rw(&locking.system_rw).is_err() {
                let lock = self.buf.pop_front().unwrap();
                self.buf.push_back(lock);

                continue;
            }

            let locking = self.buf.pop_front().unwrap();
            let params = (locking.param_getter)(state.state);

            let key = self.foreign_locks.add(locking.system_rw);
            if let Err(response) = locking.respond_to.send(ParamsResponse {
                params,
                key: ForeignParamsKey(key),
            }) {
                eprintln!("WARNING: failed to respond to foreign param request");
                self.release(response.key, state.locks);
            }
        }
    }
}
