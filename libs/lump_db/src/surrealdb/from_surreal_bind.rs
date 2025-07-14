use serde::{Deserialize, de::DeserializeOwned};

use super::{SurrealRecord, record_serde::ThingOwned};
pub use lump_db_derive::FromSurrealBind;

pub trait FromSurrealBind: 'static + Send {
    type Bind: DeserializeOwned + 'static + Send;
    fn from_bind(bind: Self::Bind) -> Self;
}

pub struct SurrealVec<T: FromSurrealBind>(Vec<T>);

impl<T: FromSurrealBind> FromSurrealBind for Vec<T> {
    type Bind = SurrealVec<T>;
    fn from_bind(bind: Self::Bind) -> Self {
        bind.0
    }
}

impl<'de, T: FromSurrealBind> Deserialize<'de> for SurrealVec<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec: Vec<T::Bind> = Deserialize::deserialize(deserializer)?;
        Ok(SurrealVec(vec.into_iter().map(T::from_bind).collect()))
    }
}

impl<R: SurrealRecord> FromSurrealBind for R {
    type Bind = ThingOwned<R>;
    fn from_bind(bind: Self::Bind) -> Self {
        R::from_owned(bind)
    }
}

macro_rules! from_bind {
    ($type:ty) => {
        impl FromSurrealBind for $type {
            type Bind = $type;
            fn from_bind(bind: Self::Bind) -> Self {
                bind
            }
        }
    };
}

from_bind!(String);
from_bind!(u128);
from_bind!(i128);
from_bind!(bool);
from_bind!(i8);
from_bind!(i16);
from_bind!(i32);
from_bind!(i64);
from_bind!(u8);
from_bind!(u16);
from_bind!(u32);
from_bind!(u64);
from_bind!(f32);
from_bind!(f64);
#[cfg(feature = "chrono")]
from_bind!(chrono::DateTime<chrono::Utc>);

