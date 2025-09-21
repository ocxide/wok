use crate::any_handle::{AnyHandle, Handle, HandleMut};
use std::{any::TypeId, collections::HashMap};
pub use wok_derive::Resource;

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

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub struct ResourceId(TypeId);

impl ResourceId {
    pub fn new<T: Resource>() -> Self {
        Self(TypeId::of::<T>())
    }
}

pub trait Resource: Sized + Send + 'static + Sync {
    type Mutability: ResourceMutability;
}

pub trait ResourceMutability {}

pub struct Immutable;
impl ResourceMutability for Immutable {}

pub struct Mutable;
impl ResourceMutability for Mutable {}
