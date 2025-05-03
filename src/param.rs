use std::{ops::Deref, sync::Arc};

use crate::{Dust, Resource};

pub struct In<T>(pub T);

pub trait Param: Send + 'static {
    fn get(dust: &Dust) -> Self;
}

impl Param for () {
    fn get(_dust: &Dust) -> Self {}
}

// macro_rules! impl_param {
//      ($($params:ident),*) => {
//          impl<$($params),*> Param for ($($params),*)
//          where
//              $($params: Param),*
//          {
//              type Param<'p> = ($($params::Param<'p>),*);
//
//              #[allow(clippy::needless_lifetimes)]
//              fn from_dust<'p>(dust: &'p Dust) -> Self::Param<'p> {
//                  ($($params::from_dust(dust)),*)
//              }
//          }
//      };
//  }
//
// impl_param!(A, B);
// impl_param!(A, B, C);
// impl_param!(A, B, C, D);
// impl_param!(A, B, C, D, E);
// impl_param!(A, B, C, D, E, F);
// impl_param!(A, B, C, D, E, F, G);
// impl_param!(A, B, C, D, E, F, G, H);
// impl_param!(A, B, C, D, E, F, G, H, I);
// impl_param!(A, B, C, D, E, F, G, H, I, J);

pub struct Res<R: Resource>(Arc<R>);

impl<R: Resource> Deref for Res<R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<R: Resource + Clone> Param for Res<R> {
    fn get(dust: &Dust) -> Self {
        let value: &R = dust.resources.get().expect("resource not found");
        let arc = Arc::new(value.clone());

        Res(arc)
    }
}
