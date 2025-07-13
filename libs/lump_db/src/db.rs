use lump::prelude::LumpUnknownError;

use crate::{RecordGenerate, data_wrappers::KeyValue};

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

pub trait DbSelectSingle<R, D>: 'static {
    type SelectQuery<'q>: Query<Option<D>>;
    fn select<'q>(&'q self, table: &'static str, id: R) -> Self::SelectQuery<'q>;
}
