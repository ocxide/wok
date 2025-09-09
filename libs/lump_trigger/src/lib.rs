use futures::{StreamExt, channel::mpsc};
use lump_core::{
    prelude::{DynSystem, In, IntoSystem, Resource, System},
    runtime::RuntimeAddon,
    schedule::{ScheduleConfigure, ScheduleLabel},
    world::{ConfigureWorld, SystemId},
};

pub trait Event: Send + Sync + 'static {}

pub struct EventTrigger<T: Event> {
    sender: mpsc::Sender<T>,
}

impl<T: Event> EventTrigger<T> {
    pub fn trigger(&mut self, event: T) {
        let _ = self.sender.try_send(event);
    }
}

impl<E: Event> Resource for EventTrigger<E> {}

struct EventHandler<E: Event>(SystemId, DynSystem<In<E>, ()>);
impl<E: Event> Resource for EventHandler<E> {}

pub struct Events;
impl ScheduleLabel for Events {}
impl<E: Event, Marker, S> ScheduleConfigure<S, (E, Marker)> for Events
where
    S: IntoSystem<Marker>,
    S::System: System<In = In<E>, Out = ()>,
{
    fn add(self, world: &mut lump_core::world::World, system: S) {
        let system = system.into_system();
        let id = world.register_system(&system);
        world.insert_resource(EventHandler(id, Box::new(system)));
    }
}

pub struct LumpTriggerRuntime<T: Event> {
    rx: mpsc::Receiver<T>,
    pending: Option<T>,
    handler: EventHandler<T>,
}
impl<T: Event> RuntimeAddon for LumpTriggerRuntime<T> {
    fn create(state: &mut lump_core::prelude::WorldState) -> Self {
        let (sx, rx) = mpsc::channel(4);

        state.resources.insert(EventTrigger { sender: sx });

        let Some(handler) = state.try_take_resource::<EventHandler<T>>() else {
            panic!(
                "Event handler of type `{}` was not registered",
                std::any::type_name::<T>()
            );
        };

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
        state: &mut lump_core::system_locking::RemoteWorldMut<'_>,
    ) {
        let Some(event) = self.pending.take() else {
            return;
        };

        let result = state.try_run(self.handler.0, &self.handler.1, event);
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
