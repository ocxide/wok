use std::sync::mpsc::{Receiver, Sender, channel};

use crate::{
    param::Param,
    prelude::Resource,
    world::{WorldMut, WorldState},
};

pub type DynCommand = Box<dyn Command>;

pub fn commands() -> (CommandSender, CommandsReceiver) {
    let (sender, receiver) = channel();
    (CommandSender(sender), CommandsReceiver(receiver))
}

#[derive(Clone)]
pub struct CommandSender(Sender<DynCommand>);

pub struct CommandsReceiver(pub(crate) Receiver<DynCommand>);

impl CommandsReceiver {
    pub fn recv(&mut self) -> impl Iterator<Item = DynCommand> {
        self.0.try_iter()
    }
}

pub struct Commands<'s> {
    sender: CommandSender,
    // other senders allow to use lifetimes
    _marker: std::marker::PhantomData<&'s ()>,
}

impl Commands<'_> {
    pub fn add(&mut self, command: impl Command + 'static) {
        self.sender
            .0
            .send(Box::new(command))
            .expect("Failed to send command");
    }

    pub fn insert_resource<R: Resource>(&mut self, resource: R) {
        self.add(InsertResource(resource));
    }
}

impl<'s> Param for Commands<'s> {
    type Owned = CommandSender;
    type AsRef<'r> = Commands<'r>;

    fn init(_rw: &mut crate::world::access::SystemLock) {}

    fn get(world: &WorldState) -> Self::Owned {
        world.commands_sx.clone()
    }

    fn from_owned(owned: &Self::Owned) -> Self::AsRef<'_> {
        Commands {
            sender: owned.clone(),
            _marker: std::marker::PhantomData,
        }
    }

    fn get_ref<'r>(world: &'r WorldState) -> Self::AsRef<'r> {
        Commands {
            sender: world.commands_sx.clone(),
            _marker: std::marker::PhantomData,
        }
    }
}

pub trait Command: Send {
    fn apply(self: Box<Self>, world: WorldMut<'_>);
}

pub struct InsertResource<R: Resource>(R);

impl<R: Resource> Command for InsertResource<R> {
    fn apply(self: Box<Self>, world: WorldMut<'_>) {
        world.state.resources.insert(self.0);
    }
}
