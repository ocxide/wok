use wok::prelude::WokUnknownError;
use surrealdb::{Connection, Surreal};

use crate::db::{DbCreate, DbDelete, DbDeleteError, DbList, DbSelectSingle};

use super::{
    AsSurrealBind, FromSurrealBind, SurrealDb, SurrealRecord, SurrealSerialize,
    record_serde::ThingOwned,
};

impl<C, R, D> DbCreate<R, D> for SurrealDb<C>
where
    C: Connection,
    D: AsSurrealBind,
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
    async fn execute(self) -> Result<R, WokUnknownError> {
        let data = SurrealSerialize(self.data);
        let mut response = self
            .db
            .query("LET $response = CREATE type::table($table) CONTENT $data")
            .query("RETURN $response.id")
            .bind(("table", self.table))
            .bind(("data", data))
            .await
            .map_err(WokUnknownError::new)?;

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
    D: FromSurrealBind,
{
    async fn execute(self) -> Result<Vec<D>, WokUnknownError> {
        let response: Vec<D::Bind> = self.db.select(self.table.to_owned()).await?;
        Ok(response.into_iter().map(D::from_bind).collect())
    }
}

impl<C, D> DbList<D> for SurrealDb<C>
where
    C: Connection,
    D: FromSurrealBind,
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

pub struct SurrealDelete<'db, C: Connection, R: SurrealRecord> {
    db: &'db Surreal<C>,
    table: &'static str,
    id: R,
}

impl<'db, C: Connection, R: SurrealRecord> crate::db::Query<Result<(), DbDeleteError>>
    for SurrealDelete<'db, C, R>
{
    async fn execute(self) -> Result<Result<(), DbDeleteError>, WokUnknownError> {
        let response = self
            .db
            .query("DELETE ONLY type::thing($table, $id) RETURN 0")
            .bind(("table", self.table))
            .bind(("id", self.id))
            .await?
            .check();

        let result = match response {
            Ok(_) => Ok(()),
            Err(_) => {
                Err(DbDeleteError::None)
            }
        };

        Ok(result)
    }
}

pub struct SurrealSelectSingle<'db, C: Connection, R> {
    db: &'db Surreal<C>,
    table: &'static str,
    id: R,
}

impl<'db, C: Connection, D, R> crate::db::Query<Option<D>> for SurrealSelectSingle<'db, C, R>
where
    D: FromSurrealBind,
    R: SurrealRecord,
{
    async fn execute(self) -> Result<Option<D>, WokUnknownError> {
        let mut response = self
            .db
            .query("SELECT * FROM type::thing($table, $id)")
            .bind(("table", self.table))
            .bind(("id", self.id))
            .await?;

        let result: Option<D::Bind> = response.take(0)?;
        Ok(result.map(D::from_bind))
    }
}

impl<C: Connection, D, R> DbSelectSingle<R, D> for SurrealDb<C>
where
    D: FromSurrealBind,
    R: SurrealRecord,
{
    type SelectQuery<'q> = SurrealSelectSingle<'q, C, R>;

    fn select<'q>(&'q self, table: &'static str, id: R) -> Self::SelectQuery<'q> {
        SurrealSelectSingle {
            db: &self.0,
            table,
            id,
        }
    }
}
