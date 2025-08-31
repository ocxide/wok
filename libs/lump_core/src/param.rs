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
    fn get_ref<'r>(world: &'r WorldState) -> Self::AsRef<'r>;
}

impl Param for () {
    type Owned = ();
    type AsRef<'r> = ();

    fn init(_rw: &mut SystemLock) {}
    fn get(_world: &WorldState) -> Self::Owned {}
    fn from_owned(_world: &()) -> Self::AsRef<'_> {}
    fn get_ref<'r>(_world: &'r WorldState) -> Self::AsRef<'r> {}
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

        fn get_ref(world: &WorldState) -> Self::AsRef<'_> {
            #[allow(non_snake_case)]
            let ($($params),*) = ($($params::get_ref(world)),*);
            ($($params),*)
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

    fn get_ref<'r>(world: &'r WorldState) -> Self::AsRef<'r> {
        let handle = world.resources.handle_ref();
        match handle {
            Some(handle) => Res(handle.read().expect("to read")),
            None => panic!(
                "Resource of type `{}` was not registered",
                std::any::type_name::<R>()
            ),
        }
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

    fn get_ref<'r>(world: &'r WorldState) -> Self::AsRef<'r> {
        let handle = world.resources.handle_ref();
        match handle {
            Some(handle) => ResMut(handle.write().expect("to write")),
            None => panic!(
                "Resource of type `{}` was not registered",
                std::any::type_name::<R>()
            ),
        }
    }
}

impl<R: Resource> Param for Option<Res<'_, R>> {
    type Owned = Option<AnyHandle<R>>;
    type AsRef<'r> = Option<Res<'r, R>>;

    fn init(rw: &mut SystemLock) {
        Res::<'_, R>::init(rw);
    }

    fn get(world: &WorldState) -> Self::Owned {
        world.try_get_resource()
    }

    fn from_owned(owned: &Self::Owned) -> Self::AsRef<'_> {
        owned.as_ref().map(Res::from_owned)
    }

    fn get_ref<'r>(world: &'r WorldState) -> Self::AsRef<'r> {
        let handle = world.resources.handle_ref();
        handle.map(|handle| Res(handle.read().expect("to read")))
    }
}

impl<R: Resource> Param for Option<ResMut<'_, R>> {
    type Owned = Option<AnyHandle<R>>;
    type AsRef<'r> = Option<ResMut<'r, R>>;

    fn init(rw: &mut SystemLock) {
        ResMut::<'_, R>::init(rw);
    }

    fn get(world: &WorldState) -> Self::Owned {
        world.try_get_resource()
    }

    fn from_owned(owned: &Self::Owned) -> Self::AsRef<'_> {
        owned.as_ref().map(ResMut::from_owned)
    }

    fn get_ref<'r>(world: &'r WorldState) -> Self::AsRef<'r> {
        let handle = world.resources.handle_ref();
        handle.map(|handle| ResMut(handle.write().expect("to write")))
    }
}
