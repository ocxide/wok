use std::{borrow::Cow, collections::HashMap, error::Error as StdError};
use valigate_derive::{Valid, impl_gates_tup};

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

impl<E: StdError + 'static> CollectsErrors for E {
    type Errors = FieldErrors;

    fn collect_errs(self, errors: &mut Self::Errors) {
        errors.0.push(Box::new(self));
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

#[derive(Default)]
pub struct FieldErrors(Vec<Box<dyn StdError>>);

impl FieldErrors {
    pub fn push<E: StdError + 'static>(&mut self, err: E) {
        self.0.push(Box::new(err));
    }

    pub fn from_one<E: StdError + 'static>(err: E) -> Self {
        Self(vec![Box::new(err)])
    }
}

impl From<FieldErrors> for Error {
    fn from(value: FieldErrors) -> Self {
        Error::Field(value)
    }
}

#[derive(Default)]
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

pub enum Error {
    Field(FieldErrors),
    Map(MapErrors),
}

#[derive(Hash, Eq, PartialEq)]
pub enum ErrorKey {
    Index(usize),
    Field(Cow<'static, str>),
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
    pub struct MinErr {
        pub min: usize,
        pub len: usize,
    }

    pub struct Min(pub usize);

    impl<T: Count> Gate<T> for Min {
        type Out = T;
        type Err = MinErr;

        fn parse(self, input: T) -> super::GateResult<Self::Out, Self::Err> {
            if input.count() < self.0 {
                let actual_len = input.count();

                return super::GateResult::ErrPass(
                    input,
                    MinErr {
                        min: self.0,
                        len: actual_len,
                    },
                );
            }

            crate::GateResult::Ok(input)
        }
    }

    pub struct Max(pub usize);

    #[derive(thiserror::Error, Debug)]
    #[error("len must be at most {max} but was {len}")]
    pub struct MaxErr {
        pub max: usize,
        pub len: usize,
    }

    impl<T: Count> Gate<T> for Max {
        type Out = T;
        type Err = MaxErr;

        fn parse(self, input: T) -> super::GateResult<Self::Out, Self::Err> {
            if input.count() > self.0 {
                let actual_len = input.count();

                return super::GateResult::ErrPass(
                    input,
                    MaxErr {
                        max: self.0,
                        len: actual_len,
                    },
                );
            }

            crate::GateResult::Ok(input)
        }
    }
}

mod valids {
    use std::collections::HashMap;

    use crate::{Error, ErrorKey, Valid};

    macro_rules! impl_valid {
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
}
