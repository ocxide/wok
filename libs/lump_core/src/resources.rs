use crate::{
    any_handle::{AnyHandle, HandleLock},
    local_any_handle::LocalAnyHandle,
};
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

    pub fn handle_ref<R: Resource>(&self) -> Option<&AnyHandle<R>> {
        // Safety: The type is guaranteed to be R
        unsafe {
            self.0
                .get(&TypeId::of::<R>())
                .map(|handle| handle.unchecked_downcast_ref())
        }
    }

    pub fn init<R: Resource + Default>(&mut self) {
        self.insert(R::default());
    }

    pub fn get_mut<R: Resource>(&mut self) -> Option<HandleLock<'_, R>> {
        let handle = self.0.get_mut(&TypeId::of::<R>())?;
        // Safety: The type is guaranteed to be R
        let write = unsafe { handle.unchecked_downcast_ref::<R>() }
            .write()
            .expect("to write");

        Some(write)
    }
}

pub trait Resource: Sized + Send + 'static + Sync {
    fn id() -> ResourceId {
        ResourceId(TypeId::of::<Self>())
    }
}

pub trait LocalResource: Sized + Send + 'static {
    fn id() -> ResourceId {
        ResourceId(TypeId::of::<Self>())
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub struct ResourceId(TypeId);

#[derive(Default)]
pub struct LocalResources(HashMap<ResourceId, LocalAnyHandle>);

impl LocalResources {
    pub fn insert<R: LocalResource>(&mut self, value: R) {
        self.0.insert(R::id(), LocalAnyHandle::new_any(value));
    }

    pub fn try_take<R: LocalResource>(&mut self) -> Option<R> {
        self.0.remove(&R::id()).and_then(|handle| handle.try_take())
    }

    pub fn init<R: LocalResource + Default>(&mut self) {
        self.insert(R::default());
    }

    pub fn get_mut<R: LocalResource>(&mut self) -> Option<&mut R> {
        self.0.get_mut(&R::id()).and_then(|handle| handle.get_mut())
    }

    pub fn get<R: LocalResource>(&self) -> Option<&R> {
        self.0.get(&R::id()).and_then(|handle| handle.get())
    }
}
