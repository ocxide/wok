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

pub mod db;
pub mod surrealdb;
