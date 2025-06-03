use std::str::FromStr;

use dust::error::DustUnknownError;
use serde::{Serialize, de::DeserializeOwned};
use surrealdb::{Connection, Surreal};

use crate::{
    Record,
    db::{DbDelete, DbDeleteError, DbList, DbOwnedCreate, DbSelectSingle, NamedBind},
};

pub struct SurrealDb<C: Connection>(Surreal<C>);

impl<C: Connection> SurrealDb<C> {
    #[inline]
    pub fn new(db: Surreal<C>) -> Self {
        SurrealDb(db)
    }
}

impl<C: Connection> dust::prelude::Resource for SurrealDb<C> {}

impl<C, R, D> DbOwnedCreate<R, D> for SurrealDb<C>
where
    C: Connection,
    D: Serialize + 'static + Send,
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

impl<'db, C: Connection, D, R: SurrealRecord> crate::db::Query<R> for SurrealCreate<'db, C, D>
where
    D: Serialize + 'static + Send,
{
    async fn execute(self) -> Result<R, dust::error::DustUnknownError> {
        let mut response = self
            .db
            .query("LET $response = CREATE type::table($table) CONTENT $data")
            .query("RETURN $response.id")
            .bind(("table", self.table))
            .bind(("data", self.data))
            .await
            .map_err(DustUnknownError::new)?;

        let id: Option<surrealdb::sql::Thing> = response.take(1)?;
        let id = id.expect("Id");
        let id = <R::IdKind as IdKind<R>>::IdWrapper::try_from(id.id)?;

        Ok(id.take_into())
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

impl<C: Connection, R: Record + Serialize> DbDelete<R> for SurrealDb<C> {
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

impl<'db, C: Connection, R: Record + Serialize> crate::db::Query<Result<(), DbDeleteError>>
    for SurrealDelete<'db, C, R>
{
    async fn execute(self) -> Result<Result<(), DbDeleteError>, dust::error::DustUnknownError> {
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

pub struct SurrealSelectSingle<'db, C: Connection, D, W> {
    db: &'db Surreal<C>,
    table: &'static str,
    condition: W,
    _phantom: std::marker::PhantomData<fn(D)>,
}

impl<'de, C: Connection, D, W> crate::db::Query<Option<D>> for SurrealSelectSingle<'de, C, D, W>
where
    D: DeserializeOwned + NamedBind,
    W: Serialize + NamedBind + 'static + Send + Sync,
{
    async fn execute(self) -> Result<Option<D>, dust::error::DustUnknownError> {
        let mut response = self
            .db
            .query(format!(
                "SELECT {} FROM type::table($table) WHERE {}=$cond",
                D::NAME,
                W::NAME
            ))
            .bind(("table", self.table))
            .bind(("cond", self.condition))
            .await?;

        let result: Option<D> = response.take(D::NAME)?;
        Ok(result)
    }
}

impl<C: Connection, D, W> DbSelectSingle<D, W> for SurrealDb<C>
where
    D: DeserializeOwned + NamedBind + 'static,
    W: Serialize + NamedBind + 'static + Send + Sync,
{
    type SelectQuery<'q> = SurrealSelectSingle<'q, C, D, W>;

    fn select<'q>(&'q self, table: &'static str, condition: W) -> Self::SelectQuery<'q> {
        SurrealSelectSingle {
            db: &self.0,
            table,
            condition,
            _phantom: std::marker::PhantomData,
        }
    }
}

pub trait SurrealRecord: Record {
    type IdKind: IdKind<Self>;
}

impl<R: SurrealRecord> NamedBind for R {
    const NAME: &'static str = "id";
}

pub trait IdKind<I> {
    type IdWrapper: TryFrom<surrealdb::sql::Id, Error: std::error::Error + Send + Sync + Sized + 'static>
        + TakeInto<I>;
}

pub struct IdString;

pub struct IdStringKind<I>(I);

#[derive(Debug)]
pub enum IdKindErr<E: std::error::Error + Send + Sync + Sized + 'static> {
    Inner(E),
    WrongType {
        expected: &'static str,
        actual: &'static str,
    },
}

impl<E: std::error::Error + Send + Sync + Sized + 'static> std::fmt::Display for IdKindErr<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdKindErr::Inner(e) => write!(f, "{}", e),
            IdKindErr::WrongType { expected, actual } => {
                write!(f, "Expected {} but got {}", expected, actual)
            }
        }
    }
}
impl<E: std::error::Error + Send + Sync + Sized + 'static> std::error::Error for IdKindErr<E> {}

impl<I: FromStr<Err: std::error::Error + Send + Sync + Sized + 'static>> TryFrom<surrealdb::sql::Id>
    for IdStringKind<I>
{
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
    I: FromStr<Err: std::error::Error + Send + Sync + Sized + 'static>,
{
    type IdWrapper = IdStringKind<I>;
}
