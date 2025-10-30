use std::{borrow::Cow, collections::HashMap, error::Error as StdError};
pub use valigate_derive::Valid;
use valigate_derive::impl_gates_tup;

pub enum GateResult<T, E> {
    Ok(T),
    ErrCut(E),
    ErrPass(T, E),
}

pub trait Gate<I> {
    type Out;
    type Err;

    fn parse(self, input: I) -> GateResult<Self::Out, Self::Err>;
}

impl_gates_tup!();

#[derive(Debug, thiserror::Error)]
#[error("field `{0}` is missing")]
pub struct MissingField(pub &'static str);

#[derive(Default)]
pub enum MaybeFieldError<E> {
    Invalid(E),
    Missing,
    #[default]
    None,
}

pub struct GateErrors<E>(pub E);

pub trait CollectsErrors {
    type Errors: sealed::ErrorsSealed + Into<crate::Error> + Default;
    fn collect_errs(self, errors: &mut Self::Errors);
}

mod sealed {
    use crate::{FieldErrors, MapErrors};

    pub trait ErrorsSealed {}
    impl ErrorsSealed for MapErrors {}
    impl ErrorsSealed for FieldErrors {}
}

pub struct FieldErr<E: StdError + 'static>(pub E);

impl<E: StdError + Sync + Send + 'static> CollectsErrors for E {
    type Errors = FieldErrors;

    fn collect_errs(self, errors: &mut Self::Errors) {
        errors.0.push(AnyError(Box::new(self)));
    }
}

macro_rules! collect_errs {
    ($first_err : ident, $($err: ident),*) => {
        #[allow(unused_parens)]
        impl<$first_err: CollectsErrors, $($err : CollectsErrors<Errors = $first_err::Errors>),*> CollectsErrors for GateErrors<(Option<$first_err>, $(Option<$err>),*)>
        {
            type Errors = $first_err::Errors;
            fn collect_errs(self, errors: &mut Self::Errors) {
                #[allow(non_snake_case)]
                let (err0, $($err),*) = self.0;

                if let Some(err) = err0 {
                    err.collect_errs(errors);
                }

                $(if let Some(err) = $err {
                    err.collect_errs(errors);
                })*
            }
        }
    };
}

collect_errs!(E0,);
collect_errs!(E0, E1);
collect_errs!(E0, E1, E2);
collect_errs!(E0, E1, E2, E3);
collect_errs!(E0, E1, E2, E3, E4);
collect_errs!(E0, E1, E2, E3, E4, E5);
collect_errs!(E0, E1, E2, E3, E4, E5, E6);
collect_errs!(E0, E1, E2, E3, E4, E5, E6, E7);
collect_errs!(E0, E1, E2, E3, E4, E5, E6, E7, E8);
collect_errs!(E0, E1, E2, E3, E4, E5, E6, E7, E8, E9);

pub trait Valid: Sized {
    type In;
    fn parse(input: Self::In) -> Result<Self, Error>;
}

pub trait GatedField: Valid {
    type Gate: Gate<Self::In>;
}

#[derive(Debug)]
pub struct AnyError(Box<dyn StdError + Send + Sync>);

impl serde::Serialize for AnyError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

#[derive(Default, Debug, serde::Serialize)]
pub struct FieldErrors(Vec<AnyError>);

impl FieldErrors {
    pub fn push<E: StdError + Send + Sync + 'static>(&mut self, err: E) {
        self.0.push(AnyError(Box::new(err)));
    }

    pub fn from_one<E: StdError + Send + Sync + 'static>(err: E) -> Self {
        Self(vec![AnyError(Box::new(err))])
    }
}

impl From<FieldErrors> for Error {
    fn from(value: FieldErrors) -> Self {
        Error::Field(value)
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct MapErrors(HashMap<ErrorKey, Error>);

impl MapErrors {
    pub fn insert_index(&mut self, index: usize, err: Error) {
        self.0.insert(ErrorKey::Index(index), err);
    }

    pub fn insert_field(&mut self, key: impl Into<Cow<'static, str>>, err: Error) {
        self.0.insert(ErrorKey::Field(key.into()), err);
    }
}

impl From<MapErrors> for Error {
    fn from(value: MapErrors) -> Self {
        Error::Map(value)
    }
}

#[derive(Debug, serde::Serialize)]
pub enum Error {
    Field(FieldErrors),
    Map(MapErrors),
}

#[derive(Debug, thiserror::Error)]
pub struct ErrorDisplay(pub Error);

impl std::fmt::Display for ErrorDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn display(
            error: &Error,
            f: &mut std::fmt::Formatter<'_>,
            depth: usize,
        ) -> std::fmt::Result {
            match error {
                Error::Field(errors) => {
                    for err in &errors.0 {
                        write!(f, "{}; ", err.0)?;
                    }
                }

                Error::Map(errors) => {
                    for (key, err) in &errors.0 {
                        write!(f, "\n{}{}: ", "\t".repeat(depth), key)?;
                        display(err, f, depth + 1)?;
                    }
                }
            }

            Ok(())
        }

        display(&self.0, f, 0)
    }
}

#[derive(Debug, Hash, Eq, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum ErrorKey {
    Index(usize),
    Field(Cow<'static, str>),
}

impl std::fmt::Display for ErrorKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKey::Index(i) => write!(f, "{}", i),
            ErrorKey::Field(field) => write!(f, "{}", field),
        }
    }
}

pub fn field_pipe<In, G>(input: In, gate: G) -> Result<G::Out, Error>
where
    G: Gate<In, Err: CollectsErrors>,
{
    match gate.parse(input) {
        GateResult::Ok(out) => Ok(out),
        GateResult::ErrCut(err) => {
            let mut errors = <G::Err as CollectsErrors>::Errors::default();
            err.collect_errs(&mut errors);
            Err(errors.into())
        }
        GateResult::ErrPass(_, err) => {
            let mut errors = <G::Err as CollectsErrors>::Errors::default();
            err.collect_errs(&mut errors);
            Err(errors.into())
        }
    }
}

pub mod gates {
    use crate::Gate;

    pub struct NoopField;
    impl<T> Gate<T> for NoopField {
        type Out = T;
        type Err = std::convert::Infallible;
        fn parse(self, input: T) -> crate::GateResult<Self::Out, Self::Err> {
            crate::GateResult::Ok(input)
        }
    }

    pub trait Count {
        fn count(&self) -> usize;
    }

    impl Count for String {
        fn count(&self) -> usize {
            self.chars().count()
        }
    }

    impl<T> Count for Vec<T> {
        fn count(&self) -> usize {
            self.len()
        }
    }

    impl<T> Count for Box<[T]> {
        fn count(&self) -> usize {
            self.len()
        }
    }

    #[derive(thiserror::Error, Debug)]
    #[error("len must be at least {min} but was {len}")]
    pub struct MinLenErr {
        pub min: usize,
        pub len: usize,
    }

    pub struct MinLen(pub usize);

    impl<T: Count> Gate<T> for MinLen {
        type Out = T;
        type Err = MinLenErr;

        fn parse(self, input: T) -> super::GateResult<Self::Out, Self::Err> {
            if input.count() < self.0 {
                let actual_len = input.count();

                return super::GateResult::ErrPass(
                    input,
                    MinLenErr {
                        min: self.0,
                        len: actual_len,
                    },
                );
            }

            crate::GateResult::Ok(input)
        }
    }

    pub struct MaxLen(pub usize);

    #[derive(thiserror::Error, Debug)]
    #[error("len must be at most {max} but was {len}")]
    pub struct MaxLenErr {
        pub max: usize,
        pub len: usize,
    }

    impl<T: Count> Gate<T> for MaxLen {
        type Out = T;
        type Err = MaxLenErr;

        fn parse(self, input: T) -> super::GateResult<Self::Out, Self::Err> {
            if input.count() > self.0 {
                let actual_len = input.count();

                return super::GateResult::ErrPass(
                    input,
                    MaxLenErr {
                        max: self.0,
                        len: actual_len,
                    },
                );
            }

            crate::GateResult::Ok(input)
        }
    }

    pub struct LessThan(pub usize);

    #[derive(thiserror::Error, Debug)]
    #[error("must be less than {less_than}")]
    pub struct LessThanErr {
        pub less_than: usize,
    }

    impl<T: PartialOrd<usize>> Gate<T> for LessThan {
        type Out = T;
        type Err = LessThanErr;

        fn parse(self, input: T) -> crate::GateResult<Self::Out, Self::Err> {
            if input < self.0 {
                return crate::GateResult::ErrPass(input, LessThanErr { less_than: self.0 });
            }

            crate::GateResult::Ok(input)
        }
    }
}

mod valids {
    use std::collections::HashMap;

    use crate::{Error, ErrorKey, Valid};

    macro_rules! impl_valid {
        (no: where $($tp: ident : $pt: path),*: for $ty:ty) => {
            impl< $($tp: $pt),* > Valid for $ty {
                type In = $ty;
                fn parse(input: Self::In) -> Result<Self, crate::Error> {
                    Ok(input)
                }
            }
        };

        (no: $ty:ty) => {
            impl Valid for $ty {
                type In = $ty;
                fn parse(input: Self::In) -> Result<Self, crate::Error> {
                    Ok(input)
                }
            }
        };
    }

    impl_valid!(no: usize);
    impl_valid!(no: u8);
    impl_valid!(no: u16);
    impl_valid!(no: u32);
    impl_valid!(no: u64);
    impl_valid!(no: u128);

    impl_valid!(no: isize);
    impl_valid!(no: i8);
    impl_valid!(no: i16);
    impl_valid!(no: i32);
    impl_valid!(no: i64);
    impl_valid!(no: i128);

    impl_valid!(no: String);
    impl_valid!(no: Box<str>);

    impl_valid!(no: std::path::PathBuf);

    impl_valid!(no: std::time::Duration);
    impl_valid!(no: std::time::Instant);
    impl_valid!(no: std::time::SystemTime);

    impl_valid!(no: std::net::SocketAddr);

    impl<V: Valid> Valid for Vec<V> {
        type In = Vec<V::In>;
        fn parse(input: Self::In) -> Result<Self, crate::Error> {
            let mut values = Vec::with_capacity(input.len());
            let mut iter = input.into_iter().enumerate();

            let (i, error) = 'anyerror: {
                for (i, value) in &mut iter {
                    match V::parse(value) {
                        Ok(value) => {
                            values.push(value);
                        }
                        Err(err) => {
                            break 'anyerror (i, err);
                        }
                    }
                }

                return Ok(values);
            };

            let mut errors = HashMap::default();
            errors.insert(ErrorKey::Index(i), error);

            errors.extend(iter.filter_map(|(i, value)| match V::parse(value) {
                Ok(_) => None,
                Err(err) => Some((ErrorKey::Index(i), err)),
            }));

            Err(Error::Map(crate::MapErrors(errors)))
        }
    }

    #[cfg(feature = "chrono")]
    mod chrono_impl {
        use super::Valid;

        impl_valid!(no: where Tz: chrono::TimeZone: for chrono::DateTime<Tz>);
        impl_valid!(no: chrono::NaiveDate);
        impl_valid!(no: chrono::Weekday);
        impl_valid!(no: chrono::TimeDelta);
        impl_valid!(no: chrono::Months);
        impl_valid!(no: chrono::Days);
        impl_valid!(no: chrono::NaiveWeek);
        impl_valid!(no: chrono::Month);
    }
}
