use crate::any_handle::AnyHandle;
use std::{any::TypeId, collections::HashMap};

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

    pub fn handle_or_else<R: Resource>(&mut self, f: impl FnOnce() -> R) -> AnyHandle<R> {
        let handle = self.0.entry(TypeId::of::<R>()).or_insert_with(|| {
            let value = f();
            AnyHandle::new_any(value)
        });

        // Safety: The type is guaranteed to be R

        unsafe { handle.clone().unchecked_downcast::<R>() }
    }

    pub fn try_take<R: Resource>(&mut self) -> Option<R> {
        self.0.remove(&TypeId::of::<R>()).and_then(|handle| {
            let handle = unsafe { handle.unchecked_downcast::<R>() };
            handle.try_take()
        })
    }

    pub fn init<R: Resource + Default>(&mut self) {
        self.insert(R::default());
    }
}

pub trait Resource: Sized + Send + Sync + 'static {}
