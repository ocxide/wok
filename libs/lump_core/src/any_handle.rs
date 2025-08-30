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

    pub fn try_take(self) -> Option<T> {
        let mut this = std::mem::ManuallyDrop::new(self.0);
        // get_mut guarantees that there is only one owner
        let this = Arc::get_mut(&mut this)?;

        // SAFETY: The type is guaranteed to be T
        let value_ref = unsafe { &mut *(this.get_mut() as *mut dyn Any as *mut T) };
        // SAFETY: We is the only owner
        let value = unsafe { std::ptr::read(value_ref) };

        Some(value)
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

    /// SAFETY: The caller must the correct type
    pub unsafe fn unchecked_downcast_ref<O: Send + Sync + 'static>(&self) -> &AnyHandle<O> {
        // Safety: The caller must the correct type
        unsafe { std::mem::transmute(self) }
    }

    #[cfg(test)]
    fn reference_count(&self) -> usize {
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
    use std::{sync::atomic::AtomicU8, thread::sleep, time::Duration};

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
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

    #[test]
    fn simple_take() {
        let handle = AnyHandle::new_any(SomeStruct { value: 12 });
        let handle = unsafe { handle.unchecked_downcast::<SomeStruct>() };

        let value = handle.try_take().expect("take failed");
        assert_eq!(value.value, 12);
    }

    #[test]
    fn cannot_take() {
        let handle = AnyHandle::new_any(SomeStruct { value: 12 });
        let handle = unsafe { handle.unchecked_downcast::<SomeStruct>() };

        for _ in 0..2 {
            let handle_two = handle.clone();
            std::thread::spawn(move || {
                sleep(Duration::from_millis(100));
                let _taken = handle_two.try_take();
            })
            .join()
            .unwrap();
        }

        assert!(handle.try_take().is_none(), "should not be able to take");
    }

    struct DropCounter {
        data: AtomicU8,
    }

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.data.fetch_add(1, std::sync::atomic::Ordering::Release);
        }
    }

    #[test]
    fn does_not_drop() {
        let handle = AnyHandle::new_any(DropCounter { data: 0.into() });
        let handle = unsafe { handle.unchecked_downcast::<DropCounter>() };

        let counter = handle.try_take().expect("take failed");
        assert_eq!(counter.data.load(std::sync::atomic::Ordering::Acquire), 0);
    }
}
