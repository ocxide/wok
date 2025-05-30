use std::hash::Hash;

use crate::commands::{self, CommandSender, CommandsReceiver};
use crate::prelude::Resource;
use crate::resources::Resources;
use crate::system::{DynSystem, IntoSystem, System};

pub struct Dust {
    pub resources: Resources,
    commands_buf: CommandsReceiver,
    pub(crate) commands_sx: CommandSender,
    systems: Vec<DynSystem<(), ()>>,
}

impl Dust {
    pub fn tick_commands(&mut self) {
        loop {
            match self.commands_buf.0.try_recv() {
                Ok(command) => command.apply(self),
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    eprintln!("WARNING: Commands buffer disconnected");
                    return;
                }
            };
        }
    }

    pub fn add_system<S, Marker>(&mut self, system: S) -> SystemId
    where
        S: IntoSystem<Marker>,
        S::System: System<In = (), Out = ()>,
    {
        self.systems.push(Box::new(system.into_system()));

        SystemId(self.systems.len())
    }
}

impl Default for Dust {
    fn default() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel::<commands::DynCommand>();

        Self {
            resources: Resources::default(),
            commands_buf: CommandsReceiver::new(receiver),
            commands_sx: CommandSender::new(sender),
            systems: Vec::new(),
        }
    }
}

#[allow(dead_code)]
fn dust_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Dust>();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SystemId(usize);

pub trait ConfigureDust: Sized {
    fn dust(&mut self) -> &mut Dust;

    fn insert_resource<R: Resource>(mut self, resource: R) -> Self {
        self.dust().resources.insert(resource);
        self
    }
}
