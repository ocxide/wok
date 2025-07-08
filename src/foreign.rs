// Request resources at runtime

use std::{any::Any, collections::VecDeque};

use futures::{
    SinkExt, StreamExt,
    channel::{mpsc, oneshot},
};
use lump_core::{
    prelude::Param,
    world::{SystemLock, SystemLocks, WorldState},
};

struct ParamsResponse {
    params: Box<dyn Any + Send>,
    key: ForeignParamsKey,
}

struct RuntimeResourcesLocker {
    param_getter: fn(&WorldState) -> Box<dyn Any + Send>,
    system_rw: SystemLock,
    respond_to: oneshot::Sender<ParamsResponse>,
}

pub struct ParamsClient {
    requester: mpsc::Sender<RuntimeResourcesLocker>,
    close_sender: mpsc::Sender<ForeignParamsKey>,
}

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
    pub fn as_ref(&self) -> P::AsRef<'_> {
        P::as_ref(&self.params)
    }
}

impl ParamsClient {
    pub async fn get<P: Param>(&mut self) -> ParamGuard<P> {
        let mut lock = SystemLock::default();
        P::init(&mut lock);

        let (sx, rx) = oneshot::channel();

        let locker = RuntimeResourcesLocker {
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

pub(crate) struct ParamsLender {
    requests: mpsc::Receiver<RuntimeResourcesLocker>,
    buf: VecDeque<RuntimeResourcesLocker>,
    foreign_locks: UnorderedQueue<SystemLock>,
}

pub(crate) struct ParamsLenderPorts {
    pub(crate) close_sender: mpsc::Receiver<ForeignParamsKey>,
}

pub(crate) struct ParamsLenderBuilder {
    pub(crate) lender: ParamsLender,
    pub(crate) client: ParamsClient,
    pub(crate) ports: ParamsLenderPorts,
}

impl Default for ParamsLenderBuilder {
    fn default() -> Self {
        let (lender, rx, client) = ParamsLender::new();
        Self {
            lender,
            client,
            ports: ParamsLenderPorts { close_sender: rx },
        }
    }
}

impl ParamsLender {
    pub fn new() -> (Self, mpsc::Receiver<ForeignParamsKey>, ParamsClient) {
        let (requester, requests) = mpsc::channel(1);
        let (close_sender, close_receiver) = mpsc::channel(1);

        (
            ParamsLender {
                requests,
                buf: VecDeque::new(),
                foreign_locks: UnorderedQueue::default(),
            },
            close_receiver,
            ParamsClient {
                requester,
                close_sender,
            },
        )
    }

    pub async fn tick(&mut self) -> Option<()> {
        let locking = self.requests.next().await?;
        self.buf.push_back(locking);

        Some(())
    }

    pub fn try_respond(&mut self, locks: &mut SystemLocks, state: &WorldState) {
        while let Some(locking) = self.buf.iter().next() {
            if locks.try_lock_rw(&locking.system_rw).is_err() {
                let lock = self.buf.pop_front().unwrap();
                self.buf.push_back(lock);

                continue;
            }

            let locking = self.buf.pop_front().unwrap();
            let params = (locking.param_getter)(state);

            let key = self.foreign_locks.add(locking.system_rw);
            if let Err(response) = locking.respond_to.send(ParamsResponse {
                params,
                key: ForeignParamsKey(key),
            }) {
                eprintln!("WARNING: failed to respond to foreign param request");
                self.release(response.key, locks);
            }
        }
    }

    pub fn release(&mut self, key: ForeignParamsKey, locks: &mut SystemLocks) {
        if let Some(lock) = self.foreign_locks.remove_at(key.0) {
            locks.release_rw(&lock);
        }
    }
}
