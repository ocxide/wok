use futures::{StreamExt, channel::mpsc};
use lump_core::{
    prelude::{DynSystem, In, Resource},
    runtime::RuntimeAddon,
    world::SystemId,
};

pub trait Event: Send + Sync + 'static {}

pub struct EventTrigger<T: Event> {
    sender: mpsc::Sender<T>,
}

struct EventHandler<E: Event>(SystemId, DynSystem<In<E>, ()>);
impl<E: Event> Resource for EventHandler<E> {}

impl<E: Event> Resource for EventTrigger<E> {}

pub struct LumpTriggerRuntime<T: Event> {
    rx: mpsc::Receiver<T>,
    pending: Option<T>,
    handler: EventHandler<T>,
}
impl<T: Event> RuntimeAddon for LumpTriggerRuntime<T> {
    fn create(state: &mut lump_core::prelude::WorldState) -> Self {
        let (sx, rx) = mpsc::channel(4);

        state.resources.insert(EventTrigger { sender: sx });

        let handler = state
            .try_take_resource::<EventHandler<T>>()
            .expect("event handler not registered");

        LumpTriggerRuntime {
            rx,
            handler,
            pending: None,
        }
    }

    async fn tick(&mut self) -> Option<()> {
        if self.pending.is_some() {
            return Some(());
        }

        let event = self.rx.next().await?;
        self.pending = Some(event);

        Some(())
    }

    fn act(
        &mut self,
        async_executor: &impl lump_core::async_executor::AsyncExecutor,
        state: &mut lump_core::system_locking::StateLocker<'_>,
    ) {
        let Some(event) = self.pending.take() else { return };

        let result = state.run_task(self.handler.0, &self.handler.1, event);
        match result {
            Ok(fut) => {
                std::mem::drop(async_executor.spawn(fut));
            }
            Err(event) => {
                self.pending = Some(event);
            }
        }

    }
}
