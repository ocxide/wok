use std::{ops::Deref, sync::Arc, task::Poll};

use futures::{
    StreamExt,
    channel::mpsc::{Receiver, Sender, channel},
};
use lump_core::{
    prelude::{Param, Res, SystemInput},
    resources::{LocalResource, LocalResources},
    schedule::{ScheduleConfigure, ScheduleLabel, Systems},
    world::{ConfigureWorld, WorldState},
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

pub struct EventSenderRes<E: Event> {
    sender: Sender<E>,
}

impl<E: Event> crate::prelude::Resource for EventSenderRes<E> {}

impl<E: Event> Clone for EventSenderRes<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

pub struct EventSender<'e, E: Event> {
    sender: EventSenderRes<E>,
    _marker: std::marker::PhantomData<&'e ()>,
}

impl<'e, E: Event> lump_core::prelude::Param for EventSender<'e, E> {
    type Owned = <Res<'e, EventSenderRes<E>> as Param>::Owned;
    type AsRef<'p> = EventSender<'p, E>;

    fn init(_: &mut lump_core::world::SystemLock) {}

    fn get(world: &lump_core::prelude::WorldState) -> Self::Owned {
        Res::get(world)
    }

    fn from_owned(owned: &Self::Owned) -> Self::AsRef<'_> {
        EventSender {
            sender: Res::<EventSenderRes<E>>::from_owned(owned).deref().clone(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<E: Event> EventSender<'_, E> {
    pub fn send(&mut self, event: E) {
        let _ = self.sender.sender.try_send(event);
    }
}

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
    fn add<Marker>(
        world: &mut lump_core::world::World,
        system: impl lump_core::prelude::IntoSystem<
            Marker,
            System: lump_core::prelude::System<In = OnEvents<'c, E>, Out = ()>,
        >,
    ) {
        let system = system.into_system();
        let systemid = world.register_system(&system);

        let Some(systems) = world.center.resources.get_mut::<EventHandlers<'_, E>>() else {
            panic!("events `{}` is not registered", std::any::type_name::<E>());
        };

        systems.add(systemid, Box::new(system), EventsBuffer(Default::default()));
    }
}

impl Events {
    pub fn register<C: RuntimeConfig, E: Event>(app: &mut AppBuilder<C>) {
        let (sx, rx) = channel::<E>(10);
        let socket = EventsSocket { recv: rx };
        let sender = EventSenderRes { sender: sx };

        app.world_mut()
            .center
            .resources
            .init::<EventHandlers<'_, E>>();
        app.world_mut().center.resources.insert(socket);
        app.world_mut().state.resources.insert(sender);

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
