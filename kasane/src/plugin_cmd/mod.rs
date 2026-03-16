mod build;
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
    }
}
