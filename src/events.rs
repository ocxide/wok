use std::{sync::Arc, task::Poll};

use futures::{StreamExt, channel::mpsc::Receiver};
use lump_core::{
    prelude::{DynSystem, SystemInput},
    resources::{LocalResource, LocalResources},
    schedule::{ScheduleConfigure, ScheduleLabel, Systems},
    world::WorldState,
};

use crate::{
    app::AppBuilder,
    runtime::{RuntimeConfig, SystemTaskLauncher},
};

type EventHandlers<'e, E> = Systems<OnEvents<'e, E>, (), EventsBuffer<E>>;

pub(crate) struct EventsBuffer<E: Event>(Vec<Arc<E>>);

struct EventsSocket<E: Event> {
    recv: Receiver<E>,
}

impl<E: Event> LocalResource for EventsSocket<E> {}

#[derive(Copy, Clone)]
pub struct Events;
impl ScheduleLabel for Events {}

pub trait Event: Send + Sync + 'static {}

pub struct OnEvents<'c, E: Event> {
    // TODO: use a reference to a central buffer
    buff: Vec<Arc<E>>,
    _marker: std::marker::PhantomData<&'c ()>,
}

impl<'c, E: Event> OnEvents<'c, E> {
    pub fn iter(&self) -> impl Iterator<Item = &E> {
        self.buff.iter().map(|arc| arc.as_ref())
    }
}

impl<'c, E: Event> SystemInput for OnEvents<'c, E> {
    type Inner<'i> = OnEvents<'i, E>;
    type Wrapped<'i> = OnEvents<'i, E>;

    fn wrap(this: Self::Inner<'_>) -> Self::Wrapped<'_> {
        this
    }
}

impl<'c, E: Event> ScheduleConfigure<OnEvents<'c, E>, ()> for Events
where
    OnEvents<'c, E>: SystemInput + 'static,
{
    fn add(
        world: &mut lump_core::world::World,
        systemid: lump_core::world::SystemId,
        system: DynSystem<OnEvents<'c, E>, ()>,
    ) {
        let Some(systems) = world.center.resources.get_mut::<EventHandlers<'c, E>>() else {
            panic!("events `{}` is not registered", std::any::type_name::<E>());
        };

        systems.add(systemid, system, EventsBuffer(Default::default()));
    }
}

impl Events {
    pub fn register<C: RuntimeConfig, E: Event>(app: &mut AppBuilder<C>) {
        app.invokers.add(Self::try_invoke::<C, E>);

        app.invokers.add_polling(
            |cx, resources| {
                let rx = resources
                    .get_mut::<EventsSocket<E>>()
                    .expect("Event to be registered");

                let event = match rx.recv.poll_next_unpin(cx) {
                    Poll::Ready(Some(event)) => event,
                    Poll::Ready(None) => return Poll::Ready(None),
                    Poll::Pending => return Poll::Pending,
                };

                let event = Arc::new(event);
                let handlers = resources
                    .get_mut::<EventHandlers<E>>()
                    .expect("Event to be registered");

                for (_, _, pending) in handlers.0.iter_mut() {
                    pending.0.push(event.clone());
                }

                Poll::Ready(Some(()))
            },
            Self::try_invoke::<C, E>,
        );
    }

    pub(crate) fn try_invoke<C: RuntimeConfig, E: Event>(
        spawner: &mut SystemTaskLauncher<'_, C>,
        resources: &mut LocalResources,
        state: &WorldState,
    ) {
        let handlers = resources
            .get_mut::<EventHandlers<E>>()
            .expect("Event to be registered");

        for (systemid, system, buffer) in handlers
            .iter_mut()
            .filter(|(_, _, pending)| !pending.0.is_empty())
        {
            let Ok(spawner) = spawner.single(systemid) else {
                continue;
            };

            let buffer = std::mem::take(&mut buffer.0);
            let input = OnEvents {
                buff: buffer,
                _marker: std::marker::PhantomData,
            };

            let task = system.run(state, input);
            spawner.spawn(task);
        }
    }
}
