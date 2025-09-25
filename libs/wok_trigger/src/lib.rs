use futures::{StreamExt, channel::mpsc};
use wok_core::{
    prelude::{BorrowTaskSystem, In, IntoSystem, Resource, System},
    runtime::RuntimeAddon,
    schedule::{ScheduleConfigure, ScheduleLabel},
    world::{gateway::TaskSystemEntry, ConfigureWorld},
};

pub trait Event: Send + Sync + 'static {}

#[derive(Resource)]
#[resource(usage = lib)]
pub struct EventTrigger<T: Event> {
    sender: mpsc::Sender<T>,
}

// impl Clone mannually to avoid Clone requirement on E
impl<E: Event> Clone for EventTrigger<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<T: Event> EventTrigger<T> {
    pub fn trigger(&mut self, event: T) {
        let _ = self.sender.try_send(event);
    }
}

#[derive(Resource)]
#[resource(usage = lib)]
struct EventHandler<E: Event>(TaskSystemEntry<In<E>, ()>);

pub struct Events;
impl ScheduleLabel for Events {}
impl<E: Event, Marker, S> ScheduleConfigure<S, (E, Marker)> for Events
where
    S: IntoSystem<Marker>,
    S::System: System<In = In<E>, Out = ()> + BorrowTaskSystem,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = world.register_system(system.into_system()).into_taskbox();
        world.insert_resource(EventHandler(system));
    }
}

pub struct WokTriggerRuntime<T: Event> {
    rx: mpsc::Receiver<T>,
    pending: Option<T>,
    handler: EventHandler<T>,
}
impl<T: Event> RuntimeAddon for WokTriggerRuntime<T> {
    type Rests = ();
    fn create(state: &mut wok_core::prelude::WorldState) -> (Self, ()) {
        let (sx, rx) = mpsc::channel(4);

        state.resources.insert(EventTrigger { sender: sx });

        let Some(handler) = state.take_resource::<EventHandler<T>>() else {
            panic!(
                "Event handler of type `{}` was not registered",
                std::any::type_name::<T>()
            );
        };

        (
            WokTriggerRuntime {
                rx,
                handler,
                pending: None,
            },
            (),
        )
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
        async_executor: &impl wok_core::async_executor::AsyncExecutor,
        state: &mut wok_core::world::gateway::RemoteWorldMut<'_>,
    ) {
        let Some(event) = self.pending.take() else {
            return;
        };

        let result = state.try_run(self.handler.0.entry_ref(), event);
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
