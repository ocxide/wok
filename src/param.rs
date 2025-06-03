use std::ops::Deref;

use crate::{
    prelude::{World, Resource},
    any_handle::{AnyHandle, HandleRead},
};

pub struct In<T>(pub T);

impl<T> Deref for In<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub trait Param: Send {
    type Owned: Send + 'static;
    type AsRef<'r>;

    fn get(world: &World) -> Self::Owned;
    fn as_ref(owned: &Self::Owned) -> Self::AsRef<'_>;
}

impl Param for () {
    type Owned = ();
    type AsRef<'r> = ();

    fn get(_world: &World) -> Self::Owned {}
    fn as_ref(_world: &()) -> Self::AsRef<'_> {}
}

macro_rules! impl_param {
    ($($params:ident),*) => {
    impl<$($params),*> Param for ($($params),*)
    where
        $($params: Param),*
    {
        type Owned = ($($params::Owned),*);
        type AsRef<'p> = ($($params::AsRef<'p>),*);

        fn get(world: &World) -> Self::Owned {
            ($($params::get(world)),*)
        }

        #[allow(clippy::needless_lifetimes)]
        fn as_ref(owned: &Self::Owned) -> Self::AsRef<'_> {
            #[allow(non_snake_case)]
            let ($($params),*) = owned;
            ($($params::as_ref($params)),*)
        }
     }
     };
 }

impl_param!(A, B);
impl_param!(A, B, C);
impl_param!(A, B, C, D);
impl_param!(A, B, C, D, E);
impl_param!(A, B, C, D, E, F);
impl_param!(A, B, C, D, E, F, G);
impl_param!(A, B, C, D, E, F, G, H);
impl_param!(A, B, C, D, E, F, G, H, I);
impl_param!(A, B, C, D, E, F, G, H, I, J);

pub struct Res<'r, R: Resource>(HandleRead<'r, R>);

impl<R: Resource> Deref for Res<'_, R> {
    type Target = R;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<R: Resource> Param for Res<'_, R> {
    type Owned = AnyHandle<R>;
    type AsRef<'r> = Res<'r, R>;

    fn get(world: &World) -> Self::Owned {
        world.resources.handle().expect("resource not found")
    }

    fn as_ref(handle: &Self::Owned) -> Self::AsRef<'_> {
        Res(handle.read().expect("to read"))
    }
}
