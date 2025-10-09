use std::ops::{Deref, DerefMut};

use crate::{
    any_handle::{Handle, HandleMut},
    prelude::{Immutable, Resource},
    resources::{Mutable, ResourceId},
    world::{UnsafeMutState, UnsafeWorldState, access::SystemLock},
};
use wok_derive::Param;

/// # Safety
/// Caller must ensure the access is indeed read-only
pub trait ReadonlyParam: BorrowMutParam {}

/// # Safety
/// Caller must ensure this `Param` will not remove / insert resources
pub unsafe trait BorrowMutParam: Param {
    /// # Safety
    /// The caller must ensure that no duplicated mutable access is happening
    unsafe fn borrow_owned(state: &UnsafeWorldState) -> Self::Owned {
        // # Safety
        // We know this param does not remove / insert resources
        unsafe { Self::get_owned(state.as_unsafe_mut()) }
    }

    /// # Safety
    /// The caller must ensure that no duplicated mutable access is happening
    unsafe fn borrow(state: &UnsafeWorldState) -> Self::AsRef<'_> {
        // # Safety
        // We know this param does not remove / insert resources
        unsafe { Self::get_ref(state.as_unsafe_mut()) }
    }
}

pub trait Param: Send {
    type Owned: Sync + Send + 'static;
    type AsRef<'r>: Param;

    fn init(rw: &mut SystemLock);
    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_>;

    /// # Safety
    /// Caller must ensure that no duplicated mutable access is happening
    unsafe fn get_owned(state: &UnsafeMutState) -> Self::Owned;

    /// # Safety
    /// Caller must ensure that no duplicated mutable access is happening
    unsafe fn get_ref(state: &UnsafeMutState) -> Self::AsRef<'_>;
}

impl Param for () {
    type Owned = ();
    type AsRef<'r> = ();

    fn init(_rw: &mut SystemLock) {}
    unsafe fn get_owned(_state: &UnsafeMutState) -> Self::Owned {}
    unsafe fn get_ref(_state: &UnsafeMutState) -> Self::AsRef<'_> {}
    fn from_owned(_owned: &mut Self::Owned) -> Self::AsRef<'_> {}
}

// We know this does not modify anything
unsafe impl BorrowMutParam for () {}

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

            unsafe fn get_owned(state: &UnsafeMutState) -> Self::Owned {
                unsafe { ($($params::get_owned(state)),*) }
            }

            unsafe fn get_ref(state: &UnsafeMutState) -> Self::AsRef<'_> {
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

        // Only impl if all params are BorrowMutParam
        unsafe impl<$($params),*> BorrowMutParam for ($($params),*)
        where
            $($params: BorrowMutParam),*
        {}
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

    unsafe fn get_owned(state: &UnsafeMutState) -> Self::Owned {
        let out = unsafe { <Option<Res<'_, R>> as Param>::get_owned(state) };
        match out {
            Some(handle) => handle,
            None => panic!(
                "Res<'_, {}>: Resource of type `{}` was not registered",
                std::any::type_name::<R>(),
                std::any::type_name::<R>()
            ),
        }
    }

    unsafe fn get_ref(state: &UnsafeMutState) -> Self::AsRef<'_> {
        unsafe { <Option<Res<'_, R>> as Param>::get_ref(state) }.expect("to have resource")
    }

    fn from_owned(handle: &mut Self::Owned) -> Self::AsRef<'_> {
        Res((*handle).as_ref())
    }
}

// We know Res does not modify the structure
unsafe impl<R: Resource> BorrowMutParam for Res<'_, R> {}

// # Safety
// We know the param is read-only since resource is immutable
impl<R: Resource<Mutability = Immutable>> ReadonlyParam for Res<'_, R> {}

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

    unsafe fn get_owned(state: &UnsafeMutState) -> Self::Owned {
        unsafe { <Option<ResMut<'_, R>> as Param>::get_owned(state) }.expect("to have resource")
    }

    unsafe fn get_ref(state: &UnsafeMutState) -> Self::AsRef<'_> {
        unsafe { <Option<ResMut<'_, R>> as Param>::get_ref(state) }.expect("to have resource")
    }

    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
        ResMut(owned.as_mut())
    }
}

// We know ResMut does not modify the structure
unsafe impl<R: Resource<Mutability = Mutable>> BorrowMutParam for ResMut<'_, R> {}

impl<R: Resource> Param for Option<Res<'_, R>> {
    type Owned = Option<<Res<'static, R> as Param>::Owned>;
    type AsRef<'r> = Option<Res<'r, R>>;

    fn init(rw: &mut SystemLock) {
        Res::<'_, R>::init(rw);
    }

    unsafe fn get_owned(state: &UnsafeMutState) -> Self::Owned {
        let state = state.as_read();
        unsafe { state.resource_handle() }
    }

    unsafe fn get_ref(state: &UnsafeMutState) -> Self::AsRef<'_> {
        let state = state.as_read();
        unsafe { state.get_resource() }.map(Res)
    }

    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
        owned.as_mut().map(Res::from_owned)
    }
}

// We know Res does not modify the structure
unsafe impl<R: Resource> BorrowMutParam for Option<Res<'_, R>> {}

impl<R: Resource<Mutability = Mutable>> Param for Option<ResMut<'_, R>> {
    type Owned = Option<<ResMut<'static, R> as Param>::Owned>;
    type AsRef<'r> = Option<ResMut<'r, R>>;

    fn init(rw: &mut SystemLock) {
        ResMut::<'_, R>::init(rw);
    }

    unsafe fn get_owned(state: &UnsafeMutState) -> Self::Owned {
        let state = state.as_read();
        unsafe { state.resource_handle_mut() }
    }

    unsafe fn get_ref(state: &UnsafeMutState) -> Self::AsRef<'_> {
        let state = state.as_read();
        unsafe { state.get_resource_mut() }.map(ResMut)
    }

    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
        owned.as_mut().map(ResMut::from_owned)
    }
}

// We know ResMut does not modify the structure
unsafe impl<R: Resource<Mutability = Mutable>> BorrowMutParam for Option<ResMut<'_, R>> {}

pub struct ResTake<R: Resource>(R);

impl<R: Resource> Deref for ResTake<R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<R: Resource> DerefMut for ResTake<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<R: Resource> ResTake<R> {
    pub fn into_inner(self) -> R {
        self.0
    }
}

impl<R: Resource> Param for ResTake<R> {
    type Owned = Option<R>;
    type AsRef<'r> = ResTake<R>;

    fn init(rw: &mut SystemLock) {
        if rw.register_resource_write(ResourceId::new::<R>()).is_err() {
            panic!(
                "Resource of type `{}` was already registered",
                std::any::type_name::<R>()
            );
        }
    }

    unsafe fn get_owned(state: &UnsafeMutState) -> Self::Owned {
        unsafe { state.take_resource() }
    }

    unsafe fn get_ref(state: &UnsafeMutState) -> Self::AsRef<'_> {
        unsafe { state.take_resource() }
            .map(ResTake)
            .expect("to have resource")
    }

    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
        ResTake(owned.take().expect("to have resource"))
    }
}

impl<R: Resource> Param for Option<ResTake<R>> {
    type Owned = <ResTake<R> as Param>::Owned;
    type AsRef<'r> = Option<ResTake<R>>;

    fn init(rw: &mut SystemLock) {
        ResTake::<R>::init(rw);
    }

    unsafe fn get_ref(state: &UnsafeMutState) -> Self::AsRef<'_> {
        unsafe { state.take_resource() }.map(ResTake)
    }

    unsafe fn get_owned(state: &UnsafeMutState) -> Self::Owned {
        unsafe { state.take_resource::<R>() }
    }

    fn from_owned(owned: &mut Self::Owned) -> Self::AsRef<'_> {
        owned.take().map(ResTake)
    }
}

pub struct ResMutMarker<R: Resource>(std::marker::PhantomData<fn(R)>);

impl<R: Resource> Param for ResMutMarker<R> {
    type Owned = ();
    type AsRef<'r> = ResMutMarker<R>;

    fn init(rw: &mut SystemLock) {
        if rw.register_resource_write(ResourceId::new::<R>()).is_err() {
            panic!(
                "Resource of type `{}` was already registered",
                std::any::type_name::<R>()
            );
        }
    }

    unsafe fn get_ref(_state: &UnsafeMutState) -> Self::AsRef<'_> {
        ResMutMarker(std::marker::PhantomData)
    }

    unsafe fn get_owned(_state: &UnsafeMutState) -> Self::Owned {}

    fn from_owned(_owned: &mut Self::Owned) -> Self::AsRef<'_> {
        ResMutMarker(std::marker::PhantomData)
    }
}

// We know ResInitMarker does not modify the structure
unsafe impl<R: Resource> BorrowMutParam for ResMutMarker<R> {}

#[derive(Param)]
#[param(usage = core)]
pub struct ResInit<'r, R: Resource> {
    commands: crate::commands::Commands<'r>,
    _marker: ResMutMarker<R>,
}

impl<'r, R: Resource> ResInit<'r, R> {
    pub fn init(&mut self, resource: R) {
        self.commands.insert_resource(resource);
    }
}
