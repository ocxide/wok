use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

mod param;
mod system;
pub mod system_fn;

pub mod prelude {
    pub use crate::param::*;
    pub use crate::system::*;
    pub use crate::{Dust, Resource};
}

#[derive(Default)]
pub struct Resources(HashMap<TypeId, Box<dyn Any>>);

impl Resources {
    pub fn insert<R: Resource>(&mut self, value: R) {
        self.0.insert(TypeId::of::<R>(), Box::new(value));
    }

    pub fn get<R: Resource>(&self) -> Option<&R> {
        self.0
            .get(&TypeId::of::<R>())
            .and_then(|v| v.downcast_ref())
    }
}

#[derive(Default)]
pub struct Dust {
    pub resources: Resources,
}

pub trait Resource: Send + Sync + 'static {}
