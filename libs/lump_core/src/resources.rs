use crate::{
    any_handle::{AnyHandle, HandleMut, Handle},
    local_any_handle::LocalAnyHandle,
};
use std::{any::TypeId, collections::HashMap};

#[derive(Default)]
pub struct Resources(HashMap<TypeId, AnyHandle>);

impl Resources {
    pub fn insert<R: Resource>(&mut self, value: R) {
        self.0.insert(TypeId::of::<R>(), AnyHandle::new_any(value));
    }

    pub fn handle<R: Resource>(&self) -> Option<Handle<R>> {
        Some(self.handle_ref()?.handle())
    }

    pub fn handle_mut<R: Resource>(&self) -> Option<HandleMut<R>> {
        self.handle_ref()?.handle_mut()
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

    pub fn handle_ref_mut<R: Resource>(&mut self) -> Option<&mut AnyHandle<R>> {
        // Safety: The type is guaranteed to be R
        unsafe {
            self.0
                .get_mut(&TypeId::of::<R>())
                .map(|handle| handle.unchecked_downcast_mut())
        }
    }

    pub fn init<R: Resource + Default>(&mut self) {
        self.insert(R::default());
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
