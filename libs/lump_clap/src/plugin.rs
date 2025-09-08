use clap::CommandFactory;
use lump::{app::ConfigureApp, plugin::Plugin, prelude::SystemReserver};
use lump_core::prelude::*;

use crate::{
    router::Router,
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
    reserver: SystemReserver<'_>,
    main: Option<Res<'_, MainHandler>>,
    mut command: ResMut<'_, CommandRoot>,
    router: Res<'_, Router>,
) -> Result<(), LumpUnknownError> {
    let mut command = command.0.take().expect("to have a command");

    if main.is_none() {
        command = command.subcommand_required(true);
    }

    command.build();

    let args = match command.try_get_matches() {
        Ok(args) => args,
        Err(err) => {
            println!("{}", err);
            return Ok(());
        }
    };

    if let Some(MainHandler(id, main)) = main.as_deref() {
        let res = match reserver.lock(*id).await.run_task(main, &args).await {
            Ok(res) => res,
            Err(err) => {
                println!("{}", err);
                return Ok(());
            }
        };

        return res;
    }

    let (name, mut sub_args) = match args.subcommand() {
        Some((name, args)) => (name, args),
        None => {
            return Ok(());
        }
    };

    let mut rotue = vec![name];

    while let Some((name, matches)) = sub_args.subcommand() {
        rotue.push(name);
        sub_args = matches;
    }

    if let Some((id, system)) = router.routes.get(rotue.as_slice()) {
        let permit = reserver.lock(*id).await;
        let result = match permit.run_task(system, sub_args).await {
            Ok(res) => res,
            Err(err) => {
                println!("{}", err);
                return Ok(());
            }
        };

        return result;
    }

    Ok(())
}

