use std::ops::{Deref, DerefMut};

use crate::{
    any_handle::{AnyHandle, HandleLock, HandleRead},
    prelude::Resource,
    world::{WorldState, access::SystemLock},
};

pub trait Param: Send {
    type Owned: Sync + Send + 'static;
    type AsRef<'r>;

    fn init(rw: &mut SystemLock);

    fn get(world: &WorldState) -> Self::Owned;
    fn from_owned(owned: &Self::Owned) -> Self::AsRef<'_>;
}

impl Param for () {
    type Owned = ();
    type AsRef<'r> = ();

    fn init(_rw: &mut SystemLock) {}
    fn get(_world: &WorldState) -> Self::Owned {}
    fn from_owned(_world: &()) -> Self::AsRef<'_> {}
}

macro_rules! impl_param {
    ($($params:ident),*) => {
    impl<$($params),*> Param for ($($params),*)
    where
        $($params: Param),*
    {
        type Owned = ($($params::Owned),*);
        type AsRef<'p> = ($($params::AsRef<'p>),*);

        fn init(rw: &mut SystemLock) {
            $(($params::init(rw)));*
        }

        fn get(world: &WorldState) -> Self::Owned {
            ($($params::get(world)),*)
        }

        #[allow(clippy::needless_lifetimes)]
        fn from_owned(owned: &Self::Owned) -> Self::AsRef<'_> {
            #[allow(non_snake_case)]
            let ($($params),*) = owned;
            ($($params::from_owned($params)),*)
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

impl<'r, R: Resource> AsRef<R> for Res<'r, R> {
    fn as_ref(&self) -> &R {
        &self.0
    }
}

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

    fn init(rw: &mut SystemLock) {
        if rw.register_resource_read(R::id()).is_err() {
            panic!(
                "Resource of type `{}` was already registered with access mode `Write`",
                std::any::type_name::<R>()
            );
        }
    }

    fn get(world: &WorldState) -> Self::Owned {
        world.get_resource()
    }

    fn from_owned(handle: &Self::Owned) -> Self::AsRef<'_> {
        Res(handle.read().expect("to read"))
    }
}

pub struct ResMut<'r, R: Resource>(HandleLock<'r, R>);

impl<'r, R: Resource> AsRef<R> for ResMut<'r, R> {
    fn as_ref(&self) -> &R {
        &self.0
    }
}

impl<'r, R: Resource> AsMut<R> for ResMut<'r, R> {
    fn as_mut(&mut self) -> &mut R {
        &mut self.0
    }
}

impl<R: Resource> Deref for ResMut<'_, R> {
    type Target = R;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<R: Resource> DerefMut for ResMut<'_, R> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<R: Resource> Param for ResMut<'_, R> {
    type Owned = AnyHandle<R>;
    type AsRef<'r> = ResMut<'r, R>;

    fn init(rw: &mut SystemLock) {
        if rw.register_resource_write(R::id()).is_err() {
            panic!(
                "Resource of type `{}` was already registered",
                std::any::type_name::<R>()
            );
        }
    }

    fn get(world: &WorldState) -> Self::Owned {
        world.get_resource()
    }

    fn from_owned(handle: &Self::Owned) -> Self::AsRef<'_> {
        ResMut(handle.write().expect("to write"))
    }
}
