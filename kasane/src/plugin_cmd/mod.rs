//! `kasane plugin` subcommand handlers: new, build, install, list, doctor, dev.

mod build;
mod dev;
mod doctor;
mod install;
mod list;
mod new;
mod templates;

use anyhow::Result;

use crate::cli::PluginSubcommand;

pub fn execute(cmd: PluginSubcommand) -> Result<()> {
    match cmd {
        PluginSubcommand::New { name, template } => new::run(&name, template),
        PluginSubcommand::Build { path } => build::run(path.as_deref()),
        PluginSubcommand::Install { path } => install::run(path.as_deref()),
        PluginSubcommand::List => list::run(),
        PluginSubcommand::Doctor { fix } => doctor::run(fix),
        PluginSubcommand::Dev { path, release } => dev::run(path.as_deref(), release),
    }
}
