use clap::{ArgMatches, CommandFactory};
use lump::{app::ConfigureApp, integrations::RemoteWorldRef, plugin::Plugin};
use lump_core::{prelude::*, system_locking::SystemEntryRef};

use crate::{
    router::{ClapHandler, Router},
    schedule::{CommandRoot, MainHandler},
};

pub struct ClapPlugin {
    command: clap::Command,
}

impl ClapPlugin {
    pub fn parser<F: CommandFactory>() -> Self {
        Self {
            command: F::command(),
        }
    }
}

impl Plugin for ClapPlugin {
    fn setup(self, app: impl ConfigureApp) {
        app.insert_resource(CommandRoot(Some(self.command)))
            .init_resource::<Router>();
    }
}

pub async fn clap_runtime(
    world: RemoteWorldRef<'_>,
    main: Option<Res<'_, MainHandler>>,
    mut command: ResMut<'_, CommandRoot>,
    router: Res<'_, Router>,
) -> Result<(), LumpUnknownError> {
    let mut command = command.0.take().expect("to have a command");

    if main.is_none() {
        command = command.subcommand_required(true);
    }

    let args = match command.try_get_matches() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{}", e);
            return Ok(());
        }
    };

    let Some((route, args)) = select_route(main.as_deref(), &args, router.as_ref()) else {
        return Ok(());
    };

    let world = world.upgrade().expect("to have a world");

    let result = world.reserver().reserve(route).await.task().run_dyn(args).await;
    match result {
        Ok(out) => out,
        Err(e) => {
            eprintln!("{}", e);
            Ok(())
        }
    }
}

fn select_route<'a>(
    main: Option<&'a MainHandler>,
    args: &'a ArgMatches,
    router: &'a Router,
) -> Option<(SystemEntryRef<'a, ClapHandler>, &'a ArgMatches)> {
    if let Some(MainHandler(id, main)) = main {
        let out = (SystemEntryRef::new(*id, main), args);
        return Some(out);
    }

    let (name, mut sub_args) = match args.subcommand() {
        Some((name, args)) => (name, args),
        None => return None,
    };

    let mut rotue = vec![name];

    while let Some((name, matches)) = sub_args.subcommand() {
        rotue.push(name);
        sub_args = matches;
    }

    if let Some((id, system)) = router.routes.get(rotue.as_slice()) {
        let out = (SystemEntryRef::new(*id, system), sub_args);
        return Some(out);
    }

    None
}
