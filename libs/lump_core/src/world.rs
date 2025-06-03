use crate::any_handle::AnyHandle;
use crate::commands::{self, CommandSender, CommandsReceiver};
use crate::prelude::Resource;
use crate::resources::Resources;
use crate::schedule::{LabeledScheduleSystem, ScheduleLabel};
use crate::system::{IntoSystem, System};

pub struct World {
    pub resources: Resources,
    commands_buf: CommandsReceiver,
    pub(crate) commands_sx: CommandSender,
}

impl World {
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

    /// # Panics
    /// Panics if the resource is not found
    pub fn get_resource<R: Resource>(&self) -> AnyHandle<R> {
        self.resources.handle().expect("Resource not found")
    }

    #[inline]
    pub fn try_take_resource<R: Resource>(&mut self) -> Option<R> {
        self.resources.try_take()
    }

    pub fn init_schedule<S: ScheduleLabel>(&mut self) {
        self.resources.init::<LabeledScheduleSystem<S>>();
    }
}

impl Default for World {
    fn default() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel::<commands::DynCommand>();

        Self {
            resources: Resources::default(),
            commands_buf: CommandsReceiver::new(receiver),
            commands_sx: CommandSender::new(sender),
        }
    }
}

#[allow(dead_code)]
fn world_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<World>();
}

pub trait ConfigureWorld: Sized {
    fn world_mut(&mut self) -> &mut World;
    fn world(&self) -> &World;

    fn insert_resource<R: Resource>(mut self, resource: R) -> Self {
        self.world_mut().resources.insert(resource);
        self
    }

    fn add_system<S: ScheduleLabel, Marker>(
        mut self,
        _: S,
        system: impl IntoSystem<Marker, System: System<In = S::SystenIn, Out = S::SystemOut>>,
    ) -> Self {
        let schedule = self
            .world_mut()
            .resources
            .handle::<LabeledScheduleSystem<S>>()
            .expect("Unsupported schedule");

        schedule
            .write()
            .expect("failed to write schedule")
            .schedule
            .add_system(system);

        self
    }
}
