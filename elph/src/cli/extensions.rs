use std::path::Path;

use clap::{Parser, Subcommand};

use super::help;
use crate::extensions::ExtensionHost;
use crate::platform::{AppPaths, EXIT_ERROR, EXIT_SUCCESS, ExitCode, Paths};

#[derive(Parser, Default)]
#[command(
    name = "extensions",
    about = "Manage WASM extensions (wasmtime + Component Model)",
    color = clap::ColorChoice::Auto
)]
pub struct ExtensionsArgs {
    #[command(subcommand)]
    pub command: Option<ExtensionsCommands>,
}

#[derive(Subcommand)]
pub enum ExtensionsCommands {
    /// List installed extensions
    List,
    /// Install an extension bundle from a local directory (contains extension.toml + component wasm)
    Install {
        /// Local path to extension bundle directory
        source: String,
        /// Replace existing extension
        #[arg(short, long)]
        force: bool,
    },
    /// Remove an installed extension
    Remove {
        /// Extension name
        name: String,
    },
    /// Enable a disabled extension
    Enable {
        /// Extension name
        name: String,
    },
    /// Disable an extension without uninstalling it
    Disable {
        /// Extension name
        name: String,
    },
}

pub fn handle(args: &ExtensionsArgs) -> ExitCode {
    let Some(cmd) = &args.command else {
        return help::print_subcommand_help::<ExtensionsArgs>();
    };

    let paths = match Paths::resolve() {
        Ok(paths) => paths,
        Err(error) => {
            tracing::error!(%error, "resolve paths");
            return EXIT_ERROR;
        }
    };

    let host = ExtensionHost::new();
    if let Err(error) = ExtensionHost::ensure_dirs(&paths) {
        tracing::error!(%error, "ensure extension dirs");
        return EXIT_ERROR;
    }
    if let Err(error) = host.reload(&paths, false) {
        tracing::error!(%error, "load extensions");
        return EXIT_ERROR;
    }

    match cmd {
        ExtensionsCommands::List => list_extensions(&host),
        ExtensionsCommands::Install { source, force } => install_extension(&host, &paths, source, *force),
        ExtensionsCommands::Remove { name } => remove_extension(&paths, name),
        ExtensionsCommands::Enable { name } => set_enabled(&paths, name, true),
        ExtensionsCommands::Disable { name } => set_enabled(&paths, name, false),
    }
}

fn list_extensions(host: &ExtensionHost) -> ExitCode {
    let paths = match Paths::resolve() {
        Ok(paths) => paths,
        Err(error) => {
            tracing::error!(%error, "resolve paths");
            return EXIT_ERROR;
        }
    };
    let settings = ExtensionHost::load_settings(&paths);
    let manifests = host.registry().read().extensions();
    if manifests.is_empty() {
        println!("No extensions installed.");
        println!("Global dir: {}", paths.global_extensions_dir().display());
        return EXIT_SUCCESS;
    }
    for manifest in manifests {
        let enabled = settings.is_enabled(&manifest.name) && manifest.enabled;
        println!(
            "{name} {version} [{state}] — {description}",
            name = manifest.name,
            version = manifest.version,
            state = if enabled { "enabled" } else { "disabled" },
            description = manifest.description,
        );
        for cmd in host
            .registry()
            .read()
            .commands()
            .into_iter()
            .filter(|cmd| cmd.extension == manifest.name)
        {
            println!("  /{} — {}", cmd.name, cmd.description);
        }
    }
    EXIT_SUCCESS
}

fn install_extension(host: &ExtensionHost, paths: &Paths, source: &str, force: bool) -> ExitCode {
    let source = Path::new(source);
    if !source.join("extension.toml").is_file() {
        tracing::error!(path = %source.display(), "missing extension.toml");
        return EXIT_ERROR;
    }
    match host.install_bundle(source, paths, force) {
        Ok(dest) => {
            println!("Installed extension to {}", dest.display());
            EXIT_SUCCESS
        }
        Err(error) => {
            tracing::error!(%error, "install extension");
            EXIT_ERROR
        }
    }
}

fn remove_extension(paths: &Paths, name: &str) -> ExitCode {
    let dest = paths.config_dir().join("extensions").join(name);
    if !dest.is_dir() {
        tracing::error!(name, "extension not installed");
        return EXIT_ERROR;
    }
    if let Err(error) = std::fs::remove_dir_all(&dest) {
        tracing::error!(%error, "remove extension");
        return EXIT_ERROR;
    }
    println!("Removed extension '{name}'.");
    EXIT_SUCCESS
}

fn set_enabled(paths: &Paths, name: &str, enabled: bool) -> ExitCode {
    let mut settings = ExtensionHost::load_settings(paths);
    settings.disabled.retain(|n| n != name);
    if !enabled && !settings.disabled.iter().any(|n| n == name) {
        settings.disabled.push(name.to_string());
    }
    match ExtensionHost::save_settings(paths, &settings) {
        Ok(()) => {
            println!("Extension '{name}' {}", if enabled { "enabled" } else { "disabled" });
            EXIT_SUCCESS
        }
        Err(error) => {
            tracing::error!(%error, "save extension settings");
            EXIT_ERROR
        }
    }
}
