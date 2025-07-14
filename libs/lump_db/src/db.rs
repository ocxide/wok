use lump::prelude::LumpUnknownError;

pub trait Query<O> {
    fn execute(self) -> impl Future<Output = Result<O, LumpUnknownError>> + Send;
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
