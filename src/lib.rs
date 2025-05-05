use std::{any::TypeId, collections::HashMap};

use any_handle::AnyHandle;

mod param;
mod system;
pub mod system_fn;

pub mod prelude {
    pub use crate::param::*;
    pub use crate::system::*;
    pub use crate::{Dust, Resource};
}

mod any_handle;

#[derive(Default)]
pub struct Resources(HashMap<TypeId, AnyHandle>);

impl Resources {
    pub fn insert<R: Resource>(&mut self, value: R) {
        self.0.insert(TypeId::of::<R>(), AnyHandle::new_any(value));
    }

    pub fn handle<R: Resource>(&self) -> Option<AnyHandle<R>> {
        let wrapped = self.0.get(&TypeId::of::<R>())?;
        // Safety: The type is guaranteed to be R
        let read = unsafe { wrapped.clone().unchecked_downcast::<R>() };

        Some(read)
    }
}

#[derive(Default)]
pub struct Dust {
    pub resources: Resources,
}

pub trait Resource: Send + Sync + 'static {}

pub struct DustMutations {}

pub struct MutDust<'d> {
    dust: &'d mut Dust,
    mutations: &'d mut DustMutations,
}

mod commands {
    use crate::{Dust, MutDust, param::Param};

    pub trait Command: Send + 'static {
        fn execute(self, dust: MutDust<'_>);
    }

    #[derive(Default)]
    pub struct Commands {
        commands: Vec<Box<dyn Command>>,
    }

    impl Param for Commands {
        type Owned = ();
        type AsRef<'r> = Commands;

        fn get(_: &Dust) -> Self::Owned {}
        fn as_ref(_: &Self::Owned) -> Self::AsRef<'_> {
            Commands::default()
        }
    }
}

mod callcenter {
    pub struct CallCenter;
}
