pub trait Record: Send + Sync + 'static + Clone + Copy {
    const TABLE: &'static str;
}

pub trait RecordGenerate: Record {
    fn generate() -> Self;
}

pub mod data_wrappers {
    use serde::de::DeserializeOwned;

    use crate::surrealdb::{IdKind, SurrealRecord, TakeInto};

    #[derive(serde::Serialize)]
    pub struct KeyValue<I, D> {
        pub id: I,
        #[serde(flatten)]
        pub data: D,
    }

    impl<'de, I: DeserializeOwned + SurrealRecord, B: serde::Deserialize<'de>>
        serde::Deserialize<'de> for KeyValue<I, B>
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            #[derive(serde::Deserialize)]
            struct KeyValueInner<D> {
                id: surrealdb::sql::Thing,
                #[serde(flatten)]
                data: D,
            }

            let inner = KeyValueInner::deserialize(deserializer)?;
            let id = <I::IdKind as IdKind<I>>::IdWrapper::try_from(inner.id.id)
                .map_err(serde::de::Error::custom)?;

            Ok(KeyValue {
                id: id.take_into(),
                data: inner.data,
            })
        }
    }
}

pub mod db {
    use dust::error::DustUnknownError;

    use crate::{RecordGenerate, data_wrappers::KeyValue};

    pub trait Query<O> {
        fn execute(self) -> impl Future<Output = Result<O, DustUnknownError>> + Send;
    }

    pub trait DbOwnedCreate<D>: 'static {
        type CreateQuery<'q>: Query<()>;
        fn create<'q>(&'q self, table: &'static str, data: D) -> Self::CreateQuery<'q>;
    }

    pub trait DbList<D>: 'static {
        type ListQuery<'q>: Query<Vec<D>>;
        fn list<'q>(&'q self, table: &'static str) -> Self::ListQuery<'q>;
    }

    pub trait IdStrategy<I>: Sized + Send + Sync + 'static {
        type Wrap<D>;
        fn wrap<D>(body: D) -> Self::Wrap<D>;
    }

    pub struct GenerateId;

    impl<I: RecordGenerate> IdStrategy<I> for GenerateId {
        type Wrap<D> = KeyValue<I, D>;
        fn wrap<D>(body: D) -> Self::Wrap<D> {
            KeyValue {
                id: I::generate(),
                data: body,
            }
        }
    }
}

pub mod surrealdb {
    use std::str::FromStr;

    use dust::error::DustUnknownError;
    use serde::{Serialize, de::DeserializeOwned};
    use surrealdb::{Connection, Surreal, opt::Resource};

    use crate::{
        Record,
        db::{DbList, DbOwnedCreate},
    };

    pub struct SurrealDb<C: Connection>(Surreal<C>);

    impl<C: Connection> SurrealDb<C> {
        #[inline]
        pub fn new(db: Surreal<C>) -> Self {
            SurrealDb(db)
        }
    }

    impl<C: Connection> dust::prelude::Resource for SurrealDb<C> {}

    impl<C, D> DbOwnedCreate<D> for SurrealDb<C>
    where
        C: Connection,
        D: Serialize + 'static + Send,
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

    impl<'db, C: Connection, D> crate::db::Query<()> for SurrealCreate<'db, C, D>
    where
        D: Serialize + 'static + Send,
    {
        async fn execute(self) -> Result<(), dust::error::DustUnknownError> {
            self.db
                .create(Resource::Table(self.table.to_owned()))
                .content(self.data)
                .await
                .map_err(DustUnknownError::new)?;
            Ok(())
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
        async fn execute(self) -> Result<Vec<D>, dust::error::DustUnknownError> {
            self.db
                .select(self.table.to_owned())
                .await
                .map_err(DustUnknownError::new)
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

    pub trait SurrealRecord: Record {
        type IdKind: IdKind<Self>;
    }

    pub trait IdKind<I> {
        type IdWrapper: TryFrom<surrealdb::sql::Id, Error: std::error::Error> + TakeInto<I>;
    }

    pub struct IdString;

    pub struct IdStringKind<I>(I);

    #[derive(Debug)]
    pub enum IdKindErr<E> {
        Inner(E),
        WrongType {
            expected: &'static str,
            actual: &'static str,
        },
    }

    impl<E: std::error::Error> std::fmt::Display for IdKindErr<E> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                IdKindErr::Inner(e) => write!(f, "{}", e),
                IdKindErr::WrongType { expected, actual } => {
                    write!(f, "Expected {} but got {}", expected, actual)
                }
            }
        }
    }
    impl<E: std::error::Error> std::error::Error for IdKindErr<E> {}

    impl<I: FromStr> TryFrom<surrealdb::sql::Id> for IdStringKind<I> {
        type Error = IdKindErr<I::Err>;

        fn try_from(value: surrealdb::sql::Id) -> Result<Self, Self::Error> {
            let value = match value {
                surrealdb::sql::Id::String(s) => s,
                surrealdb::sql::Id::Number(_) => {
                    return Err(IdKindErr::WrongType {
                        expected: "String",
                        actual: "Number",
                    });
                }
                _ => {
                    return Err(IdKindErr::WrongType {
                        expected: "String",
                        actual: "Unknown",
                    });
                }
            };

            let id = IdStringKind(I::from_str(&value).map_err(IdKindErr::Inner)?);
            Ok(id)
        }
    }

    pub trait TakeInto<I> {
        fn take_into(self) -> I;
    }

    impl<I> TakeInto<I> for IdStringKind<I> {
        fn take_into(self) -> I {
            self.0
        }
    }

    impl<I> IdKind<I> for IdString
    where
        I: FromStr<Err: std::error::Error>,
    {
        type IdWrapper = IdStringKind<I>;
    }
}
