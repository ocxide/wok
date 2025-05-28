use dust::{
    Resource,
    entity::Entity,
    prelude::{In, Res},
};

pub trait DbCreateFactory<E>: 'static {
    type CreateQuery<'q>: DbCreate;
    type Leftover;

    fn create<'q>(
        &'q self,
        table: &'static str,
        data: E,
    ) -> (Self::CreateQuery<'q>, Self::Leftover);
}

pub trait DbCreate {
    fn create(self) -> impl Future<Output = ()>;
}

pub trait EntityCreate: Entity {
    type Data;
    const TABLE: &'static str;
}

pub async fn create<E, Db>(entity: In<E::Data>, db: Res<'_, Db>) -> (E, Db::Leftover)
where
    E: EntityCreate,
    Db: DbCreateFactory<E::Data> + Resource,
{
    let id = E::unique_new();
    let (query, left) = db.create(E::TABLE, entity.0);

    query.create().await;
    (id, left)
}

mod surrealdb {
    use surrealdb::{Connection, opt::Resource};

    use crate::{DbCreate, DbCreateFactory};

    pub struct SurrealDb<C: Connection>(surrealdb::Surreal<C>);

    impl<E, C> DbCreateFactory<E> for SurrealDb<C>
    where
        C: Connection,
        E: serde::Serialize + 'static,
    {
        type CreateQuery<'q> = Create<'q, C, E>;
        type Leftover = ();

        fn create<'q>(
            &'q self,
            table: &'static str,
            data: E,
        ) -> (Self::CreateQuery<'q>, Self::Leftover) {
            (
                Create {
                    db: &self.0,
                    table,
                    data,
                },
                (),
            )
        }
    }

    impl<C> dust::Resource for SurrealDb<C> where C: Connection {}

    pub struct Create<'s, C: Connection, T> {
        db: &'s surrealdb::Surreal<C>,
        table: &'static str,
        data: T,
    }

    impl<'s, C: Connection, T> DbCreate for Create<'s, C, T>
    where
        T: serde::Serialize + 'static,
    {
        async fn create(self) {
            let _ = self.db
                .create(Resource::Table(self.table.to_owned()))
                .content(self.data)
                .await;
        }
    }
}
