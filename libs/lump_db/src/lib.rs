pub trait Record: Send + Sync + 'static + Clone + Copy {
    const TABLE: &'static str;
}

pub trait RecordGenerate: Record {
    fn generate() -> Self;
}

pub mod db;
#[cfg(feature = "surrealdb")]
pub mod surrealdb;

pub mod id_strategy {
    pub trait IdStrategy<I>: Sized + Send + Sync + 'static {
        type Wrap<D>;
        fn wrap<D>(body: D) -> Self::Wrap<D>;
    }

    pub struct GenerateId;
}
