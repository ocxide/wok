use std::collections::HashMap;

use clap::ArgMatches;
use wok::prelude::BorrowTaskSystem;
use wok_core::{
    prelude::{InRef, Resource, WokUnknownError},
    world::SystemId,
};

pub type ClapHandler = Box<dyn BorrowTaskSystem<In = HandlerIn, Out = HandlerOut> + Send + Sync>;
pub type HandlerIn = InRef<'static, ArgMatches>;
pub type HandlerOut = Result<Result<(), WokUnknownError>, clap::error::Error>;

#[derive(Default, Resource)]
#[resource(mutable = true)]
pub struct Router {
    pub routes: HashMap<Box<[&'static str]>, (SystemId, ClapHandler)>,
}

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
