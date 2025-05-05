use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
pub use std::any::Any;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// Mutable handle of sized static obj
pub struct AnyHandle<T: ?Sized + Sync + Send + 'static = dyn Any + Send + Sync + 'static>(
    Arc<RwLock<dyn Any + Send + Sync + 'static>>,
    PhantomData<T>,
);

pub enum AnyLockError<T: ?Sized> {
    WouldBlock,
    Poisoned(PhantomData<fn(T)>),
}

impl<T: ?Sized> Display for AnyLockError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnyLockError::WouldBlock => write!(
                f,
                "WouldBlock locking of type {}",
                std::any::type_name::<T>()
            ),
            AnyLockError::Poisoned(_) => {
                write!(f, "Poisoned lock of type {}", std::any::type_name::<T>())
            }
        }
    }
}

impl<T: ?Sized> Debug for AnyLockError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnyLockError")
            .field(
                "variant",
                match self {
                    AnyLockError::WouldBlock => &"WouldBlock",
                    AnyLockError::Poisoned(_) => &"Poisoned",
                },
            )
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}

impl<T: Sized + Sync + Send + 'static> AnyHandle<T> {
    pub fn new_any(value: T) -> AnyHandle<dyn Any + Send + Sync + 'static> {
        AnyHandle(Arc::new(RwLock::new(value)), PhantomData)
    }

    pub fn read(&self) -> Result<HandleRead<T>, AnyLockError<T>> {
        let guad = match self.0.try_read() {
            Some(guad) => guad,
            _ => return Err(AnyLockError::WouldBlock),
        };
        let read = HandleRead(guad, PhantomData);

        Ok(read)
    }

    pub fn write(&self) -> Result<HandleLock<T>, AnyLockError<T>> {
        let guad = match self.0.try_write() {
            Some(guad) => guad,
            _ => return Err(AnyLockError::WouldBlock),
        };
        let read = HandleLock(guad, PhantomData);

        Ok(read)
    }
}

impl<T: ?Sized + Sync + Send + 'static> AnyHandle<T> {
    /// Be aware, that in order to downcast, the handle should not be locked
    pub fn downcast<O: Send + Sync + 'static>(self) -> Option<AnyHandle<O>> {
        if self.0.try_read().expect("to read").is::<O>() {
            Some(unsafe { self.unchecked_downcast() })
        } else {
            None
        }
    }

    /// SAFETY: The caller must the correct type
    pub unsafe fn unchecked_downcast<O: Send + Sync + 'static>(self) -> AnyHandle<O> {
        AnyHandle(self.0, PhantomData)
    }

    pub fn reference_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

impl<T: ?Sized + Sync + Send + 'static> Clone for AnyHandle<T> {
    fn clone(&self) -> Self {
        AnyHandle(self.0.clone(), PhantomData)
    }
}

pub struct HandleRead<'r, T: Sized + Sync + Send + 'static>(
    RwLockReadGuard<'r, dyn Any + Send + Sync + 'static>,
    PhantomData<T>,
);

pub struct HandleLock<'r, T: Sized + Sync + Send + 'static>(
    RwLockWriteGuard<'r, dyn Any + Send + Sync + 'static>,
    PhantomData<T>,
);

impl<T: Sized + Sync + Send + 'static> Deref for HandleRead<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: the lock guarantees that the object is of type T
        unsafe { &*(self.0.deref() as *const dyn Any as *const T) }
    }
}

impl<T: Sized + Sync + Send + 'static> Deref for HandleLock<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: the lock guarantees that the object is of type T
        unsafe { &*(self.0.deref() as *const dyn Any as *const T) }
    }
}

impl<T: Sized + Sync + Send + 'static> DerefMut for HandleLock<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: the lock guarantees that the object is of type T
        unsafe { &mut *(self.0.deref_mut() as *mut dyn Any as *mut T) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SomeStruct {
        value: i32,
    }

    #[test]
    fn basic_reading_writing() {
        let handle = AnyHandle::new_any(SomeStruct { value: 12 });
        let handle = handle.downcast::<SomeStruct>().expect("downcast failed");

        {
            let handle_two = handle.clone();

            assert_eq!(handle.read().unwrap().value, 12);
            assert_eq!(handle_two.read().unwrap().value, 12);
            assert_eq!(handle.reference_count(), 2);
            handle.write().unwrap().value = 24;
            assert_eq!(handle.read().unwrap().value, 24);
            assert_eq!(handle_two.read().unwrap().value, 24);
        }

        assert_eq!(handle.reference_count(), 1);
    }

    #[test]
    fn multithreading() {
        let handle = AnyHandle::new_any(SomeStruct { value: 12 });
        let handle = handle.downcast::<SomeStruct>().expect("downcast failed");

        let handle_two = handle.clone();

        let handle = std::thread::spawn(move || {
            let handle = handle.downcast::<SomeStruct>().expect("downcast failed");
            assert_eq!(handle.read().unwrap().value, 12);
            assert_eq!(handle.reference_count(), 2);
            handle.write().unwrap().value = 24;
            assert_eq!(handle.read().unwrap().value, 24);
            assert_eq!(handle_two.read().unwrap().value, 24);
        });
        handle.join().unwrap();
    }
}
