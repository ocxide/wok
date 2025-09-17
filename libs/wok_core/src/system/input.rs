use std::ops::Deref;

pub trait SystemInput: Sized + Send {
    type Wrapped<'i>: SystemInput;
    type Inner<'i>: Send;

    fn wrap(this: Self::Inner<'_>) -> Self::Wrapped<'_>;
}

impl SystemInput for () {
    type Wrapped<'i> = ();
    type Inner<'i> = ();

    fn wrap(_this: Self::Inner<'_>) -> Self::Wrapped<'_> {}
}

pub struct In<T: Sized + 'static + Send>(pub T);

impl<T: Send> Deref for In<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Sized + 'static + Send> SystemInput for In<T> {
    type Wrapped<'i> = In<T>;
    type Inner<'i> = T;

    fn wrap(this: Self::Inner<'_>) -> Self::Wrapped<'_> {
        In(this)
    }
}

pub struct InRef<'i, I: ?Sized + 'static + Send>(&'i I);

impl<'i, I: ?Sized + Send> Deref for InRef<'i, I> {
    type Target = I;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<I: ?Sized + 'static + Send> SystemInput for InRef<'_, I>
where
    for<'i> &'i I: Send,
{
    type Wrapped<'i> = InRef<'i, I>;
    type Inner<'i> = &'i I;

    fn wrap(this: Self::Inner<'_>) -> Self::Wrapped<'_> {
        InRef(this)
    }
}

