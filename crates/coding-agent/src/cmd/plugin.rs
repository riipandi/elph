use clap::{Args, Subcommand};

use crate::app::{EXIT_SUCCESS, ExitCode};

#[derive(Args, Default)]
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
        eprintln!("Manage plugins and extensions");
        eprintln!();
        eprintln!("Usage: elph plugin <command>");
        eprintln!();
        eprintln!("Commands:");
        eprintln!("  list     List installed plugins");
        eprintln!("  install  Install a plugin from a git URL or local path");
        eprintln!("  remove   Remove an installed plugin");
        eprintln!("  update   Update installed plugin(s)");
        eprintln!("  enable   Enable a disabled plugin");
        eprintln!("  disable  Disable a plugin without uninstalling it");
        eprintln!("  help     Print this message or the help of the given subcommand(s)");
        return EXIT_SUCCESS;
    };
    match cmd {
        PluginCommands::List => {
            eprintln!("Plugin list — not yet implemented");
            EXIT_SUCCESS
        }
        PluginCommands::Install {
            source,
            trust,
            global,
            force,
        } => {
            eprintln!(
                "Plugin install — not yet implemented (source: {source}, trust: {trust}, global: {global}, force: {force})"
            );
            EXIT_SUCCESS
        }
        PluginCommands::Remove { name, global } => {
            eprintln!("Plugin remove — not yet implemented (name: {name}, global: {global})");
            EXIT_SUCCESS
        }
        PluginCommands::Update { name, all } => {
            eprintln!(
                "Plugin update — not yet implemented (name: {}, all: {all})",
                name.as_deref().unwrap_or("<none>")
            );
            EXIT_SUCCESS
        }
        PluginCommands::Enable { name } => {
            eprintln!("Plugin enable — not yet implemented (name: {name})");
            EXIT_SUCCESS
        }
        PluginCommands::Disable { name } => {
            eprintln!("Plugin disable — not yet implemented (name: {name})");
            EXIT_SUCCESS
        }
    }
}
