use crate::app::{EXIT_SUCCESS, ExitCode};
use crate::cli::{PluginArgs, PluginCommands};

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
