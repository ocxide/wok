use std::collections::HashMap;

use clap::ArgMatches;
use lump_core::{
    prelude::{DynSystem, InRef, LumpUnknownError, Resource},
    world::SystemId,
};

pub type ClapHandler = DynSystem<HandlerIn, HandlerOut>;
pub type HandlerIn = InRef<'static, ArgMatches>;
pub type HandlerOut = Result<Result<(), LumpUnknownError>, clap::error::Error>;

#[derive(Default)]
pub struct Router {
    pub routes: HashMap<Box<[&'static str]>, (SystemId, ClapHandler)>,
}

impl Resource for Router {}
impl Router {
    pub fn add(
        &mut self,
        route: impl Into<Box<[&'static str]>>,
        system_id: SystemId,
        handler: ClapHandler,
    ) {
        self.routes.insert(route.into(), (system_id, handler));
    }
}

