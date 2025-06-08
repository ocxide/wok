use std::any::Any;

pub struct LocalAnyHandle(Box<dyn Any>);

impl LocalAnyHandle {
    pub fn new_any<T: Sized + 'static>(value: T) -> LocalAnyHandle {
        LocalAnyHandle(Box::new(value))
    }

    pub fn try_take<T: Sized + 'static>(self) -> Option<T> {
        let boxed = self.0.downcast::<T>().ok()?;
        Some(*boxed)
    }
}
