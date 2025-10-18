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

impl<I> Gate<I> for () {
    type Out = I;
    type Err = SingleErr<std::convert::Infallible>;

    fn parse(self, input: I) -> GateResult<Self::Out, Self::Err> {
        GateResult::Ok(input)
    }
}

impl_gates_tup!();

pub struct SingleErr<E: StdError + 'static>(pub E);

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

pub trait CollectFieldErrs {
    fn collect_errs(self) -> Error;
}

impl<E: StdError + 'static> CollectFieldErrs for SingleErr<E> {
    fn collect_errs(self) -> Error {
        Error::Field(vec![Box::new(self.0) as Box<dyn StdError>])
    }
}

macro_rules! collect_errs {
    ($($err: ident),*) => {
        #[allow(unused_parens)]
        impl<$($err : StdError + 'static),*> CollectFieldErrs for ($(Option<$err>),*)
        {
            fn collect_errs(self) -> Error {
                #[allow(non_snake_case)]
                let ($($err),*) = self;
                let errors = [$($err.map(|e| Box::new(e) as Box<dyn StdError>)),*].into_iter().filter_map(|e| e).collect();
                Error::Field(errors)
            }
        }
    };
}

collect_errs!(E0);
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

pub enum Error {
    Field(Vec<Box<dyn StdError>>),
    Map(HashMap<ErrorKey, Error>),
}

#[derive(Hash, Eq, PartialEq)]
pub enum ErrorKey {
    Index(usize),
    Field(Cow<'static, str>),
}

pub fn field_pipe<In, G>(input: In, gate: G) -> Result<G::Out, Error>
where
    G: Gate<In, Err: CollectFieldErrs>,
{
    match gate.parse(input) {
        GateResult::Ok(out) => Ok(out),
        GateResult::ErrCut(err) => Err(err.collect_errs()),
        GateResult::ErrPass(_, err) => Err(err.collect_errs()),
    }
}

#[derive(Valid)]
#[valid(usage = crate, gate = (
    gates::Min(2),
    gates::Max(4),
))]
pub struct W2(String);

#[derive(Valid)]
#[valid(usage = crate)]
pub struct MyData {
    w2: W2,
}

mod gates {
    use crate::Gate;

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

            Err(Error::Map(errors))
        }
    }
}
