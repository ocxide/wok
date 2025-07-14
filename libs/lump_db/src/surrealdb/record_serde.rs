use serde::{Serialize, de::DeserializeOwned};

use crate::Record;

#[derive(Serialize)]
pub struct ThingRef<'r, R> {
    pub tb: &'r str,
    pub id: IdRef<'r, R>,
}

#[derive(Serialize)]
pub enum IdRef<'r, R> {
    String(&'r R),
}

pub trait SurrealRecord: Record + DeserializeOwned + Serialize {
    type Flavor: IdFlavor<Self>;

    fn thing_ref<'r>(&'r self) -> ThingRef<'r, Self> {
        ThingRef {
            tb: Self::TABLE,
            id: Self::Flavor::create(self),
        }
    }

    fn from_owned(thing: ThingOwned<Self>) -> Self {
        Self::Flavor::from_owned(thing.id)
    }
}

pub trait IdFlavor<R: Record> {
    fn create(id: &R) -> IdRef<R>;
    fn from_owned(id: IdOwned<R>) -> R;
}

pub struct StringFlavor;

impl<R: Record> IdFlavor<R> for StringFlavor {
    fn create(id: &R) -> IdRef<R> {
        IdRef::String(id)
    }

    fn from_owned(id: IdOwned<R>) -> R {
        match id {
            IdOwned::String(id) => id,
        }
    }
}

#[derive(serde::Deserialize)]
pub struct ThingOwned<R> {
    pub id: IdOwned<R>,
}

#[derive(serde::Deserialize)]
pub enum IdOwned<R> {
    String(R),
}

