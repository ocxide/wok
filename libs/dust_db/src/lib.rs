pub trait Record: Send + Sync + 'static + Clone + Copy {
    const TABLE: &'static str;
}

pub trait RecordGenerate: Record {
    fn generate() -> Self;
}

pub mod data_wrappers {
    #[derive(serde::Serialize, serde::Deserialize)]
    pub struct KeyValue<I, D> {
        pub id: I,
        #[serde(flatten)]
        pub data: D,
    }
}

pub mod db {
    use dust::error::DustUnknownError;

    use crate::{data_wrappers::KeyValue, RecordGenerate};

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
    use dust::error::DustUnknownError;
    use serde::{Serialize, de::DeserializeOwned};
    use surrealdb::{Connection, Surreal, opt::Resource};

    use crate::db::{DbList, DbOwnedCreate};

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
}
