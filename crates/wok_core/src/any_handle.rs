pub use std::any::Any;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

struct UnsafeAny<T: ?Sized>(UnsafeCell<T>);

unsafe impl Send for UnsafeAny<dyn Any + Send + Sync + 'static> {}
unsafe impl Sync for UnsafeAny<dyn Any + Send + Sync + 'static> {}

/// Mutable handle of sized static obj
pub struct AnyHandle<T: ?Sized + Sync + Send + 'static = dyn Any + Send + Sync + 'static>(
    Arc<UnsafeAny<dyn Any + Send + Sync + 'static>>,
    PhantomData<T>,
);

impl<T: Sized + Sync + Send + 'static> AnyHandle<T> {
    pub fn new_any(value: T) -> AnyHandle<dyn Any + Send + Sync + 'static> {
        let unsafe_any = UnsafeAny(UnsafeCell::new(value));
        AnyHandle(Arc::new(unsafe_any), PhantomData)
    }

    /// # SAFETY: The caller must guarantee that no other thread is mutating the object
    pub unsafe fn get(&self) -> &T {
        // SAFETY: the lock guarantees that the object is of type T
        let any = unsafe { &*self.0.0.get() };
        any.downcast_ref().expect("downcast failed")
    }

    fn self_clone(&self) -> AnyHandle<T> {
        AnyHandle(self.0.clone(), PhantomData)
    }

    pub fn handle(&self) -> Handle<T> {
        Handle(self.self_clone())
    }

    /// SAFETY: The caller must guarantee that no other thread is holding the object
    pub fn handle_mut(&self) -> Option<HandleMut<T>> {
        if Arc::strong_count(&self.0) == 1 {
            Some(HandleMut(self.self_clone()))
        } else {
            None
        }
    }

    pub unsafe fn get_mut(&mut self) -> &mut T {
        // SAFETY: the lock guarantees that the object is of type T
        let any = unsafe { &mut *self.0.0.get() };
        any.downcast_mut().expect("downcast failed")
    }

    pub fn try_take(self) -> Option<T> {
        let mut this = std::mem::ManuallyDrop::new(self.0);
        // get_mut guarantees that there is only one owner
        let this = Arc::get_mut(&mut this)?;

        // SAFETY: The type is guaranteed to be T
        let value_ref = unsafe { &*this.0.get() }.downcast_ref()?;
        // SAFETY: We is the only owner
        let value = unsafe { std::ptr::read(value_ref) };

        Some(value)
    }
}

impl<T: ?Sized + Sync + Send + 'static> AnyHandle<T> {
    /// SAFETY: The caller must the correct type
    pub unsafe fn unchecked_downcast<O: Send + Sync + 'static>(self) -> AnyHandle<O> {
        AnyHandle(self.0, PhantomData)
    }

    /// SAFETY: The caller must the correct type
    pub unsafe fn unchecked_downcast_ref<O: Send + Sync + 'static>(&self) -> &AnyHandle<O> {
        // Safety: The caller must the correct type
        unsafe { std::mem::transmute(self) }
    }

    /// SAFETY: The caller must the correct type
    pub unsafe fn unchecked_downcast_mut<O: Send + Sync + 'static>(&mut self) -> &mut AnyHandle<O> {
        // Safety: The caller must the correct type
        unsafe { std::mem::transmute(self) }
    }

    #[cfg(test)]
    fn reference_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

pub struct HandleMut<T: Sized + Sync + Send + 'static>(AnyHandle<T>);

impl<T: Sized + Sync + Send + 'static> Deref for HandleMut<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // Safety: Given this point its guaranteed that there is only one owner
        unsafe { self.0.get() }
    }
}

impl<T: Sized + Sync + Send + 'static> DerefMut for HandleMut<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Safety: Given this point its guaranteed that there is only one owner
        unsafe { self.0.get_mut() }
    }
}

impl<T: Sized + Sync + Send + 'static> AsRef<T> for HandleMut<T> {
    fn as_ref(&self) -> &T {
        // Safety: Given this point its guaranteed that there is only one owner
        unsafe { self.0.get() }
    }
}

impl<T: Sized + Sync + Send + 'static> AsMut<T> for HandleMut<T> {
    fn as_mut(&mut self) -> &mut T {
        // Safety: Given this point its guaranteed that there is only one owner
        unsafe { self.0.get_mut() }
    }
}

pub struct Handle<T: Sized + Sync + Send + 'static>(AnyHandle<T>);

impl<T: Sized + Sync + Send + 'static> AsRef<T> for Handle<T> {
    fn as_ref(&self) -> &T {
        // Safety: Given this point its guaranteed that there are only read owners
        unsafe { self.0.get() }
    }
}

impl<T: Sized + Sync + Send + 'static> Deref for Handle<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // Safety: Given this point its guaranteed that there are only read owners
        unsafe { self.0.get() }
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
        let handle = unsafe { handle.unchecked_downcast::<SomeStruct>() };

        {
            let handle_one = handle.handle();
            let handle_two = handle.handle();

            assert_eq!(handle_one.value, 12);
            assert_eq!(handle_two.value, 12);
        }

        {
            let mut write = handle.handle_mut().expect("mut failed");
            write.value = 13;

            assert_eq!(write.value, 13);
        }

        assert_eq!(handle.reference_count(), 1);
    }

    #[test]
    fn multithreading() {
        let handle = AnyHandle::new_any(SomeStruct { value: 12 });
        let handle = unsafe { handle.unchecked_downcast::<SomeStruct>() };

        let tr = {
            let handle = handle.handle();
            std::thread::spawn(move || {
                sleep(Duration::from_millis(100));

                assert_eq!(handle.value, 12, "tr1");
            })
        };

        let tr2 = {
            let handle_two = handle.handle();
            std::thread::spawn(move || {
                sleep(Duration::from_millis(101));

                assert_eq!(handle_two.value, 12, "tr2");
            })
        };
        tr.join().unwrap();
        tr2.join().unwrap();

        let tr3 = {
            let mut handle = handle.handle_mut().expect("mut failed");
            std::thread::spawn(move || {
                sleep(Duration::from_millis(200));

                handle.value = 1;
            })
        };

        tr3.join().unwrap();

        assert_eq!(handle.handle().value, 1);
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
