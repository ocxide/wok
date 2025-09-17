use record_serde::ThingOwned;
use surrealdb::{Connection, Surreal};

pub use as_surreal_bind::{AsSurrealBind, SurrealSerialize};
pub use from_surreal_bind::FromSurrealBind;
pub use record_serde::{IdFlavor, StringFlavor, SurrealRecord};

use crate::{
    RecordEntry, RecordGenerate,
    id_strategy::{GenerateId, IdStrategy},
};

mod as_surreal_bind;
mod crud;
mod from_surreal_bind;
mod record_serde;

pub struct SurrealDb<C: Connection>(Surreal<C>);

impl<C: Connection> SurrealDb<C> {
    #[inline]
    pub const fn new(db: Surreal<C>) -> Self {
        SurrealDb(db)
    }
}

impl<C: Connection> wok::prelude::Resource for SurrealDb<C> {}

pub struct KeyValue<R: SurrealRecord, B> {
    pub id: R,
    pub data: B,
}

#[derive(serde::Deserialize)]
pub struct FromRecordEntrySurreal<R, D> {
    pub id: R,
    #[serde(flatten)]
    pub data: D,
}

#[derive(serde::Serialize)]
pub struct SurrealKeyValueRef<'b, R: SurrealRecord, B: AsSurrealBind> {
    pub id: &'b R,
    #[serde(flatten)]
    pub data: B::Bind<'b>,
}

impl<R: SurrealRecord, B: AsSurrealBind> AsSurrealBind for KeyValue<R, B> {
    type Bind<'b> = SurrealKeyValueRef<'b, R, B>;
    fn as_bind(&self) -> Self::Bind<'_> {
        SurrealKeyValueRef {
            id: &self.id,
            data: self.data.as_bind(),
        }
    }
}

impl<R: SurrealRecord + RecordGenerate> IdStrategy<R> for GenerateId {
    type Wrap<D> = KeyValue<R, D>;
    fn wrap<D>(body: D) -> Self::Wrap<D> {
        KeyValue {
            id: R::generate(),
            data: body,
        }
    }
}

impl<R: SurrealRecord, D: AsSurrealBind> AsSurrealBind for RecordEntry<R, D> {
    type Bind<'b> = SurrealKeyValueRef<'b, R, D>;
    fn as_bind(&self) -> Self::Bind<'_> {
        SurrealKeyValueRef {
            id: &self.id,
            data: self.data.as_bind(),
        }
    }
}

impl<R: SurrealRecord, D: FromSurrealBind> FromSurrealBind for RecordEntry<R, D> {
    type Bind = FromRecordEntrySurreal<ThingOwned<R>, D::Bind>;

    fn from_bind(bind: Self::Bind) -> Self {
        Self {
            id: R::from_owned(bind.id),
            data: D::from_bind(bind.data),
        } 
    }
}
