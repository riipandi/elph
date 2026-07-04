use clap::{Parser, Subcommand};

use super::help;
use crate::runtime::{EXIT_SUCCESS, ExitCode};

#[derive(Parser, Default)]
#[command(
    name = "plugin",
    about = "Manage plugins and extensions",
    color = clap::ColorChoice::Auto
)]
pub struct PluginArgs {
    #[command(subcommand)]
    pub command: Option<PluginCommands>,
}

#[derive(Subcommand)]
pub enum PluginCommands {
    /// List installed plugins
    List,
    /// Install a plugin from a git URL or local path
    Install {
        /// Source URL, GitHub shorthand (user/repo), or local path
        source: String,
        /// Trust the plugin immediately (skip confirmation prompt)
        #[arg(long)]
        trust: bool,
        /// Install in global config instead of project-local
        #[arg(short, long)]
        global: bool,
        /// Replace existing plugin version
        #[arg(short, long)]
        force: bool,
    },
    /// Remove an installed plugin
    Remove {
        /// Name of the plugin to remove
        name: String,
        /// Remove from global config instead of project-local
        #[arg(short, long)]
        global: bool,
    },
    /// Update installed plugin(s)
    Update {
        /// Plugin name to update (updates all if omitted)
        name: Option<String>,
        /// Update all installed plugins
        #[arg(long)]
        all: bool,
    },
    /// Enable a disabled plugin
    Enable {
        /// Name of the plugin to enable
        name: String,
    },
    /// Disable a plugin without uninstalling it
    Disable {
        /// Name of the plugin to disable
        name: String,
    },
}

pub fn handle(args: &PluginArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<PluginArgs>();
    };
    match cmd {
        PluginCommands::List => {
            help::unimplemented("Plugin list — not yet implemented");
            EXIT_SUCCESS
        }
        PluginCommands::Install {
            source,
            trust,
            global,
            force,
        } => {
            help::unimplemented(&format!(
                "Plugin install — not yet implemented (source: {source}, trust: {trust}, global: {global}, force: {force})"
            ));
            EXIT_SUCCESS
        }
        PluginCommands::Remove { name, global } => {
            help::unimplemented(&format!(
                "Plugin remove — not yet implemented (name: {name}, global: {global})"
            ));
            EXIT_SUCCESS
        }
        PluginCommands::Update { name, all } => {
            help::unimplemented(&format!(
                "Plugin update — not yet implemented (name: {}, all: {all})",
                name.as_deref().unwrap_or("<none>")
            ));
            EXIT_SUCCESS
        }
        PluginCommands::Enable { name } => {
            help::unimplemented(&format!("Plugin enable — not yet implemented (name: {name})"));
            EXIT_SUCCESS
        }
        PluginCommands::Disable { name } => {
            help::unimplemented(&format!("Plugin disable — not yet implemented (name: {name})"));
            EXIT_SUCCESS
        }
    }
}
