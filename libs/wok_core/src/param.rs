use std::ops::{Deref, DerefMut};

use crate::{
    any_handle::{Handle, HandleMut},
    prelude::Resource,
    resources::{Mutable, ResourceId},
    world::{UnsafeWorldState, access::SystemLock},
};

pub trait Param: Send {
    type Owned: Sync + Send + 'static;
    type AsRef<'r>;

    fn init(rw: &mut SystemLock);

    /// # Safety
    /// Caller must ensure the access is valid
    unsafe fn get(state: &UnsafeWorldState) -> Self::Owned;
    /// # Safety
    /// Caller must ensure the access is valid
    unsafe fn get_ref(state: &UnsafeWorldState) -> Self::AsRef<'_>;
    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_>;
}

impl Param for () {
    type Owned = ();
    type AsRef<'r> = ();

    fn init(_rw: &mut SystemLock) {}
    unsafe fn get(_state: &UnsafeWorldState) -> Self::Owned {}
    unsafe fn get_ref(_state: &UnsafeWorldState) -> Self::AsRef<'_> {}
    fn from_owned(_owned: &mut Self::Owned) -> Self::AsRef<'_> {}
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

        unsafe fn get(state: &UnsafeWorldState) -> Self::Owned {
            unsafe { ($($params::get(state)),*) }
        }

        unsafe fn get_ref(state: &UnsafeWorldState) -> Self::AsRef<'_> {
            #[allow(non_snake_case)]
            let ($($params),*) = unsafe { ($($params::get_ref(state)),*) };
            ($($params),*)
        }

        fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
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

pub struct Res<'r, R: Resource>(&'r R);

impl<'r, R: Resource> AsRef<R> for Res<'r, R> {
    fn as_ref(&self) -> &R {
        self.0
    }
}

impl<R: Resource> Deref for Res<'_, R> {
    type Target = R;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<R: Resource> Param for Res<'_, R> {
    type Owned = Handle<R>;
    type AsRef<'r> = Res<'r, R>;

    fn init(rw: &mut SystemLock) {
        if rw.register_resource_read(ResourceId::new::<R>()).is_err() {
            panic!(
                "Resource of type `{}` was already registered with access mode `Write`",
                std::any::type_name::<R>()
            );
        }
    }

    unsafe fn get(state: &UnsafeWorldState) -> Self::Owned {
        unsafe { <Option<Res<'_, R>> as Param>::get(state) }.expect("to have resource")
    }

    unsafe fn get_ref(state: &UnsafeWorldState) -> Self::AsRef<'_> {
        unsafe { <Option<Res<'_, R>> as Param>::get_ref(state) }.expect("to have resource")
    }

    fn from_owned(handle: &mut Self::Owned) -> Self::AsRef<'_> {
        Res((*handle).as_ref())
    }
}

pub struct ResMut<'r, R: Resource<Mutability = Mutable>>(&'r mut R);

impl<'r, R: Resource<Mutability = Mutable>> AsRef<R> for ResMut<'r, R> {
    fn as_ref(&self) -> &R {
        self.0
    }
}

impl<'r, R: Resource<Mutability = Mutable>> AsMut<R> for ResMut<'r, R> {
    fn as_mut(&mut self) -> &mut R {
        self.0
    }
}

impl<R: Resource<Mutability = Mutable>> Deref for ResMut<'_, R> {
    type Target = R;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<R: Resource<Mutability = Mutable>> DerefMut for ResMut<'_, R> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<R: Resource<Mutability = Mutable>> Param for ResMut<'_, R> {
    type Owned = HandleMut<R>;
    type AsRef<'r> = ResMut<'r, R>;

    fn init(rw: &mut SystemLock) {
        if rw.register_resource_write(ResourceId::new::<R>()).is_err() {
            panic!(
                "Resource of type `{}` was already registered",
                std::any::type_name::<R>()
            );
        }
    }

    unsafe fn get(state: &UnsafeWorldState) -> Self::Owned {
        unsafe { <Option<ResMut<'_, R>> as Param>::get(state) }.expect("to have resource")
    }

    unsafe fn get_ref(state: &UnsafeWorldState) -> Self::AsRef<'_> {
        unsafe { <Option<ResMut<'_, R>> as Param>::get_ref(state) }.expect("to have resource")
    }

    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
        ResMut(owned.as_mut())
    }
}

impl<R: Resource> Param for Option<Res<'_, R>> {
    type Owned = Option<<Res<'static, R> as Param>::Owned>;
    type AsRef<'r> = Option<Res<'r, R>>;

    fn init(rw: &mut SystemLock) {
        Res::<'_, R>::init(rw);
    }

    unsafe fn get(state: &UnsafeWorldState) -> Self::Owned {
        unsafe { state.resource_handle() }
    }

    unsafe fn get_ref(state: &UnsafeWorldState) -> Self::AsRef<'_> {
        let state = unsafe { state.get_resource() };
        state.map(Res)
    }

    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
        owned.as_mut().map(Res::from_owned)
    }
}

impl<R: Resource<Mutability = Mutable>> Param for Option<ResMut<'_, R>> {
    type Owned = Option<<ResMut<'static, R> as Param>::Owned>;
    type AsRef<'r> = Option<ResMut<'r, R>>;

    fn init(rw: &mut SystemLock) {
        ResMut::<'_, R>::init(rw);
    }

    unsafe fn get(state: &UnsafeWorldState) -> Self::Owned {
        unsafe { state.resource_handle_mut() }
    }

    unsafe fn get_ref(state: &UnsafeWorldState) -> Self::AsRef<'_> {
        let state = unsafe { state.get_resource_mut() };
        state.map(ResMut)
    }

    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
        owned.as_mut().map(ResMut::from_owned)
    }
}
