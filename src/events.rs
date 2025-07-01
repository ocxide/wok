use std::sync::{Arc, mpsc::Receiver};

use lump_core::{
    prelude::{DynSystem, In, InRef, SystemInput},
    resources::LocalResource,
    schedule::{ScheduleConfigure, ScheduleLabel, Systems},
    world::{World, WorldCenter, WorldState},
};

use crate::app::{RuntimeConfig, SystemTaskLauncher};

#[derive(Copy, Clone)]
pub struct Events;

impl Events {
    pub fn init<C: RuntimeConfig>(world: &mut World) {
        world.center.resources.init::<RegisteredEvents<C>>();
    }

    pub fn invoke_event<E: Event, C: RuntimeConfig, In: SystemInput + 'static>(
        center: &mut WorldCenter,
        state: &WorldState,
        launcher: SystemTaskLauncher<'_, C>,
    ) where
        for<'e> &'e E: Into<In::Inner<'e>>,
    {
        let events_recv = center
            .resources
            .get::<EventList<E>>()
            .expect("Event to be registered");
        let systems = center
            .resources
            .get::<Systems<In, ()>>()
            .expect("Event to be registered");

        for event in events_recv.recv.try_iter() {
            let event = Arc::new(event);
            for (systemid, system) in systems.0.iter() {
                let event = event.clone();
                let task = system.create_task(state);

                let systemid = *systemid;

                launcher.single(async move {
                    let input = event.as_ref().into();
                    task.run(input).await;

                    systemid
                });
            }
        }
    }

    pub fn register<C: RuntimeConfig, E: Event>(world: &mut World) {
        let registered_events = world
            .center
            .resources
            .get_mut::<RegisteredEvents<C>>()
            .expect("Events schedule to be initialized");

        registered_events
            .invokers
            .push(Self::invoke_event::<E, C, InRef<E>>);
    }
}

impl ScheduleLabel for Events {}

pub trait Event: Send + Sync + 'static {
    fn as_input_ref(&self) -> <InRef<'_, Self> as SystemInput>::Inner<'_> {
        self
    }
}

impl<E: Event> ScheduleConfigure<In<&E>, ()> for Events {
    fn add(
        world: &mut lump_core::world::World,
        systemid: lump_core::world::SystemId,
        system: DynSystem<In<&E>, ()>,
    ) {
        let Some(systems) = world.center.resources.get_mut::<Systems<In<&E>, ()>>() else {
            panic!("events `{}` is not registered", std::any::type_name::<E>());
        };

        systems.add(systemid, system);
    }
}

struct EventList<E: Event> {
    recv: Receiver<E>,
}

impl<E: Event> LocalResource for EventList<E> {}

pub struct RegisteredEvents<C: RuntimeConfig> {
    invokers: Vec<fn(&mut WorldCenter, &WorldState, SystemTaskLauncher<'_, C>)>,
}

impl<C: RuntimeConfig> LocalResource for RegisteredEvents<C> {}
impl<C: RuntimeConfig> Default for RegisteredEvents<C> {
    fn default() -> Self {
        Self {
            invokers: Vec::new(),
        }
    }
}
