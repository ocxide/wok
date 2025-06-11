use futures::{
    StreamExt,
    channel::mpsc::{Receiver, Sender},
};

use crate::{
    param::Param,
    prelude::Resource,
    world::{WorldMut, WorldState},
};

pub type DynCommand = Box<dyn Command>;

pub fn commands() -> (CommandSender, CommandsReceiver) {
    let (sender, receiver) = futures::channel::mpsc::channel(31);
    (CommandSender(sender), CommandsReceiver(receiver))
}

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

    pub async fn recv(&mut self) -> Option<DynCommand> {
        self.0.next().await
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
            .try_send(Box::new(command))
            .expect("Failed to send command");
    }

    pub fn insert_resource<R: Resource>(&mut self, resource: R) {
        self.add(InsertResource(resource));
    }
}

impl<'s> Param for Commands<'s> {
    type Owned = CommandSender;
    type AsRef<'r> = Commands<'r>;

    fn init(_rw: &mut crate::world::access::SystemAccess) {}

    fn get(world: &WorldState) -> Self::Owned {
        world.commands_sx.clone()
    }

    fn as_ref(owned: &Self::Owned) -> Self::AsRef<'_> {
        Commands {
            sender: owned.clone(),
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
