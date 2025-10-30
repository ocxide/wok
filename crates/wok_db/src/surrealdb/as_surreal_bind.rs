use std::u128;

use serde::{Serialize, ser::SerializeSeq};

use super::{SurrealRecord, record_serde::ThingRef};
pub use wok_db_derive::AsSurrealBind;

/// Borrow self as a serializable type with special considerations for SurrealDB
pub trait AsSurrealBind: Send + 'static {
    type Bind<'b>: Serialize + 'b + Send;
    fn as_bind(&self) -> Self::Bind<'_>;
}

pub struct SurrealSlice<'b, T: AsSurrealBind>(&'b [T]);

impl<'b, T: AsSurrealBind> Serialize for SurrealSlice<'b, T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for item in self.0.iter() {
            seq.serialize_element(&item.as_bind())?;
        }
        seq.end()
    }
}

impl<T: AsSurrealBind + Send + Sync> AsSurrealBind for Vec<T> {
    type Bind<'b> = SurrealSlice<'b, T>;
    fn as_bind(&self) -> Self::Bind<'_> {
        SurrealSlice(self)
    }
}

impl<R: SurrealRecord + Serialize> AsSurrealBind for R {
    type Bind<'b> = ThingRef<'b, R>;
    #[inline]
    fn as_bind(&self) -> Self::Bind<'_> {
        self.thing_ref()
    }
}

macro_rules! as_bind_ref {
    ($type:ty) => {
        impl AsSurrealBind for $type {
            type Bind<'b> = &'b $type;
            fn as_bind(&self) -> Self::Bind<'_> {
                self
            }
        }
    };
}

as_bind_ref!(String);
as_bind_ref!(u128);
as_bind_ref!(i128);
as_bind_ref!(f32);
as_bind_ref!(f64);
#[cfg(feature = "chrono")]
as_bind_ref!(chrono::DateTime<chrono::Utc>);

macro_rules! as_bind_copy {
    ($type:ty) => {
        impl AsSurrealBind for $type {
            type Bind<'b> = $type;
            fn as_bind(&self) -> Self::Bind<'_> {
                *self
            }
        }
    };
}

as_bind_copy!(bool);
as_bind_copy!(i8);
as_bind_copy!(i16);
as_bind_copy!(i32);
as_bind_copy!(i64);
as_bind_copy!(isize);
as_bind_copy!(u8);
as_bind_copy!(u16);
as_bind_copy!(u32);
as_bind_copy!(u64);
as_bind_copy!(usize);

pub struct SurrealSerialize<T: AsSurrealBind>(pub T);

impl<T: AsSurrealBind> Serialize for SurrealSerialize<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.as_bind().serialize(serializer)
    }
}

