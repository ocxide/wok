use wok::prelude::WokUnknownError;

use crate::Record;

pub trait Query<O> {
    fn execute(self) -> impl Future<Output = Result<O, WokUnknownError>> + Send;
}

pub trait DbCreate<R, D>: 'static {
    type CreateQuery<'q>: Query<R>;
    fn create<'q>(&'q self, table: &'static str, data: D) -> Self::CreateQuery<'q>;
}

pub trait DbList<D>: 'static {
    type ListQuery<'q>: Query<Vec<D>>;
    fn list<'q>(&'q self, table: &'static str) -> Self::ListQuery<'q>;
}

pub enum DbDeleteError {
    None,
}

pub trait DbDelete<R>: 'static {
    type DeleteQuery<'q>: Query<Result<(), DbDeleteError>>;
    fn delete<'q>(&'q self, table: &'static str, id: R) -> Self::DeleteQuery<'q>;
}

pub trait DbSelectSingle<R, D>: 'static {
    type SelectQuery<'q>: Query<Option<D>>;
    fn select<'q>(&'q self, table: &'static str, id: R) -> Self::SelectQuery<'q>;
}

pub trait Db: Sized + 'static {
    fn record<R: Record>(&self) -> DbRecordManager<Self, R> {
        DbRecordManager {
            table: R::TABLE,
            db: self,
            _marker: std::marker::PhantomData,
        }
    }
}

pub struct DbRecordManager<'db, Db, R: Record> {
    table: &'static str,
    db: &'db Db,
    _marker: std::marker::PhantomData<R>,
}

impl<'db, Db, R: Record> DbRecordManager<'db, Db, R> {
    pub fn table(self, table: &'static str) -> DbRecordManager<'db, Db, R> {
        DbRecordManager {
            table,
            db: self.db,
            _marker: self._marker,
        }
    }

    pub fn create<D>(self, data: D) -> Db::CreateQuery<'db>
    where
        Db: DbCreate<R, D>,
    {
        self.db.create(self.table, data)
    }

    pub fn list(self) -> Db::ListQuery<'db>
    where
        Db: DbList<R>,
    {
        self.db.list(self.table)
    }

    pub fn delete(self, id: R) -> Db::DeleteQuery<'db>
    where
        Db: DbDelete<R>,
    {
        self.db.delete(self.table, id)
    }

    pub fn select(self, id: R) -> Db::SelectQuery<'db>
    where
        Db: DbSelectSingle<R, R>,
    {
        self.db.select(self.table, id)
    }
}
