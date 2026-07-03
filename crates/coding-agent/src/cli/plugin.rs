use clap::{Args, Subcommand};

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
