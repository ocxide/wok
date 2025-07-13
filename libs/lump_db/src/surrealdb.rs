use lump::prelude::LumpUnknownError;
use record_serde::ThingOwned;
use serde::de::DeserializeOwned;
use surreal_bind::{AsSurrealBind, SurrealSerialize};
use surrealdb::{Connection, Surreal};

pub use lump_db_derive::AsSurrealBind;
pub use record_serde::{IdFlavor, StringFlavor, SurrealRecord};

use crate::{
    Record,
    db::{DbCreate, DbDelete, DbDeleteError, DbList, DbSelectSingle},
};

pub struct SurrealDb<C: Connection>(Surreal<C>);

impl<C: Connection> SurrealDb<C> {
    #[inline]
    pub const fn new(db: Surreal<C>) -> Self {
        SurrealDb(db)
    }
}

impl<C: Connection> lump::prelude::Resource for SurrealDb<C> {}

impl<C, R, D> DbCreate<R, D> for SurrealDb<C>
where
    C: Connection,
    D: AsSurrealBind ,
    R: SurrealRecord,
{
    type CreateQuery<'q> = SurrealCreate<'q, C, D>;

    fn create<'q>(&'q self, table: &'static str, data: D) -> Self::CreateQuery<'q> {
        SurrealCreate {
            db: &self.0,
            table,
            data,
        }
    }
}

pub struct SurrealCreate<'db, C: Connection, D> {
    db: &'db Surreal<C>,
    table: &'static str,
    data: D,
}

impl<'db, C, D, R> crate::db::Query<R> for SurrealCreate<'db, C, D>
where
    D: AsSurrealBind + Send,
    R: SurrealRecord,
    C: Connection,
{
    async fn execute(self) -> Result<R, LumpUnknownError> {
        let data = SurrealSerialize(self.data);
        let mut response = self
            .db
            .query("LET $response = CREATE type::table($table) CONTENT $data")
            .query("RETURN $response.id")
            .bind(("table", self.table))
            .bind(("data", data))
            .await
            .map_err(LumpUnknownError::new)?;

        let id: Option<ThingOwned<R>> = response.take(1)?;
        let id = id.expect("Id");
        let id = R::from_owned(id);

        Ok(id)
    }
}

pub struct SurrealList<'db, C: Connection> {
    db: &'db Surreal<C>,
    table: &'static str,
}

impl<'db, C: Connection, D> crate::db::Query<Vec<D>> for SurrealList<'db, C>
where
    D: DeserializeOwned,
{
    async fn execute(self) -> Result<Vec<D>, LumpUnknownError> {
        self.db
            .select(self.table.to_owned())
            .await
            .map_err(LumpUnknownError::new)
    }
}

impl<C, D> DbList<D> for SurrealDb<C>
where
    C: Connection,
    D: DeserializeOwned,
{
    type ListQuery<'q> = SurrealList<'q, C>;
    fn list<'q>(&'q self, table: &'static str) -> Self::ListQuery<'q> {
        SurrealList { db: &self.0, table }
    }
}

impl<C: Connection, R: SurrealRecord> DbDelete<R> for SurrealDb<C> {
    type DeleteQuery<'q> = SurrealDelete<'q, C, R>;
    fn delete<'q>(&'q self, table: &'static str, id: R) -> Self::DeleteQuery<'q> {
        SurrealDelete {
            db: &self.0,
            table,
            id,
        }
    }
}

pub struct SurrealDelete<'db, C: Connection, R: Record> {
    db: &'db Surreal<C>,
    table: &'static str,
    id: R,
}

impl<'db, C: Connection, R: SurrealRecord> crate::db::Query<Result<(), DbDeleteError>>
    for SurrealDelete<'db, C, R>
{
    async fn execute(self) -> Result<Result<(), DbDeleteError>, LumpUnknownError> {
        let response = self
            .db
            .query("DELETE ONLY type::thing($table, $id) RETURN 0")
            .bind(("table", self.table))
            .bind(("id", self.id))
            .await?
            .check();

        let result = match response {
            Ok(_) => Ok(()),
            Err(e) => {
                println!("Failed to delete: {}", e);
                dbg!(e);
                Err(DbDeleteError::None)
            }
        };

        Ok(result)
    }
}

// pub struct SurrealSelectSingle<'db, C: Connection, R> {
//     db: &'db Surreal<C>,
//     table: &'static str,
//     id: R,
// }
//
// impl<'db, C: Connection, D, R> crate::db::Query<Option<D>> for SurrealSelectSingle<'db, C, W>
// where
//     D: DeserializeOwned,
//     R
// {
//     async fn execute(self) -> Result<Option<D>, LumpUnknownError> {
//         let mut response = self
//             .db
//             .query(format!(
//                 "SELECT {} FROM type::table($table) WHERE {}=$cond",
//                 D::NAME,
//                 W::NAME
//             ))
//             .bind(("table", self.table))
//             .bind(("cond", self.condition))
//             .await?;
//
//         let result: Option<D> = response.take(D::NAME)?;
//         Ok(result)
//     }
// }
//
// impl<C: Connection, D, W> DbSelectSingle<D, W> for SurrealDb<C>
// where
//     D: DeserializeOwned + NamedBind + 'static,
//     W: Serialize + NamedBind + 'static + Send + Sync,
// {
//     type SelectQuery<'q> = SurrealSelectSingle<'q, C, D, W>;
//
//     fn select<'q>(&'q self, table: &'static str, condition: W) -> Self::SelectQuery<'q> {
//         SurrealSelectSingle {
//             db: &self.0,
//             table,
//             condition,
//             _phantom: std::marker::PhantomData,
//         }
//     }
// }

mod surreal_bind {
    use serde::{Serialize, ser::SerializeSeq};

    use super::{SurrealRecord, record_serde::ThingRef};

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
    as_bind_copy!(u8);
    as_bind_copy!(u16);
    as_bind_copy!(u32);
    as_bind_copy!(u64);

    pub struct SurrealSerialize<T: AsSurrealBind>(pub T);

    impl<T: AsSurrealBind> Serialize for SurrealSerialize<T> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            self.0.as_bind().serialize(serializer)
        }
    }
}

mod record_serde {
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
}
