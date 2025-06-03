use std::sync::mpsc::{Receiver, Sender};

use crate::{world::World, param::Param, prelude::Resource};

pub type DynCommand = Box<dyn Command>;

#[derive(Clone)]
pub struct CommandSender(Sender<DynCommand>);

impl CommandSender {
    pub(crate) fn new(sender: Sender<DynCommand>) -> Self {
        Self(sender)
    }
}

pub struct CommandsReceiver(pub(crate) Receiver<DynCommand>);

impl CommandsReceiver {
    pub(crate) fn new(receiver: Receiver<DynCommand>) -> Self {
        Self(receiver)
    }
}

pub struct Commands<'s>(&'s CommandSender);

impl Commands<'_> {
    pub fn add(&self, command: impl Command + 'static) {
        self.0
            .0
            .send(Box::new(command))
            .expect("Failed to send command");
    }

    pub fn insert_resource<R: Resource>(&self, resource: R) {
        self.add(InsertResource(resource));
    }
}

impl<'s> Param for Commands<'s> {
    type Owned = CommandSender;
    type AsRef<'r> = Commands<'r>;

    fn get(world: &World) -> Self::Owned {
        world.commands_sx.clone()
    }

    fn as_ref(owned: &Self::Owned) -> Self::AsRef<'_> {
        Commands(owned)
    }
}

pub trait Command: Send {
    fn apply(self: Box<Self>, world: &mut World);
}

pub struct InsertResource<R: Resource>(R);

impl<R: Resource> Command for InsertResource<R> {
    fn apply(self: Box<Self>, world: &mut World) {
        world.resources.insert(self.0);
    }
}
