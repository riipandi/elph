use std::path::Path;

use clap::{Parser, Subcommand};

use super::help;
use super::style::{self, CliStyle, S_ACCENT, S_BODY, S_HEADER, S_MUTED, S_OK, S_WARN};
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
            help::cli_error(format!("resolve paths: {error}"));
            return EXIT_ERROR;
        }
    };

    let host = ExtensionHost::new();
    if let Err(error) = ExtensionHost::ensure_dirs(&paths) {
        help::cli_error(format!("ensure extension dirs: {error}"));
        return EXIT_ERROR;
    }
    if let Err(error) = host.reload(&paths, false) {
        help::cli_error(format!("load extensions: {error}"));
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
    let sty = CliStyle::auto();
    let paths = match Paths::resolve() {
        Ok(paths) => paths,
        Err(error) => {
            help::cli_error(format!("resolve paths: {error}"));
            return EXIT_ERROR;
        }
    };
    let settings = ExtensionHost::load_settings(&paths);
    let manifests = host.registry().read().extensions();
    let mut out = String::new();

    if manifests.is_empty() {
        style::info(&mut out, sty, sty.paint(S_MUTED, "No extensions installed."));
        style::kv(&mut out, sty, "Global dir", paths.global_extensions_dir().display());
        print!("{out}");
        return EXIT_SUCCESS;
    }

    style::section(&mut out, sty, &format!("Extensions ({})", manifests.len()));
    use std::fmt::Write;
    let _ = writeln!(out);

    for manifest in &manifests {
        let enabled = settings.is_enabled(&manifest.name) && manifest.enabled;
        let state = if enabled {
            sty.paint(S_OK, "enabled")
        } else {
            sty.paint(S_MUTED, "disabled")
        };
        let _ = writeln!(
            out,
            "  {}  {}  {}",
            sty.paint(S_ACCENT, &manifest.name),
            sty.paint(S_MUTED, &manifest.version),
            state,
        );
        let _ = writeln!(
            out,
            "   {}  {}",
            sty.paint(S_MUTED, "·"),
            sty.paint(S_BODY, &manifest.description)
        );

        for cmd in host
            .registry()
            .read()
            .commands()
            .iter()
            .filter(|cmd| cmd.extension == manifest.name)
        {
            let _ = writeln!(
                out,
                "   {}  {}  {}",
                sty.paint(S_HEADER, "/"),
                sty.paint(S_ACCENT, &cmd.name),
                sty.paint(S_MUTED, &cmd.description),
            );
        }
    }

    print!("{out}");
    EXIT_SUCCESS
}

fn install_extension(host: &ExtensionHost, paths: &Paths, source: &str, force: bool) -> ExitCode {
    let sty = CliStyle::auto();
    let source = Path::new(source);
    if !source.join("extension.toml").is_file() {
        eprintln!("missing extension.toml: path={}", source.display());
        return EXIT_ERROR;
    }
    match host.install_bundle(source, paths, force) {
        Ok(dest) => {
            let mut out = String::new();
            style::success(&mut out, sty, format!("Installed extension to {}", dest.display()));
            print!("{out}");
            EXIT_SUCCESS
        }
        Err(error) => {
            help::cli_error(format!("install extension: {error}"));
            EXIT_ERROR
        }
    }
}

fn remove_extension(paths: &Paths, name: &str) -> ExitCode {
    let sty = CliStyle::auto();
    let dest = paths.config_dir().join("extensions").join(name);
    if !dest.is_dir() {
        eprintln!("extension not installed: {name}");
        return EXIT_ERROR;
    }
    if let Err(error) = std::fs::remove_dir_all(&dest) {
        help::cli_error(format!("remove extension: {error}"));
        return EXIT_ERROR;
    }
    let mut out = String::new();
    style::success(&mut out, sty, format!("Removed extension '{name}'."));
    print!("{out}");
    EXIT_SUCCESS
}

fn set_enabled(paths: &Paths, name: &str, enabled: bool) -> ExitCode {
    let sty = CliStyle::auto();
    let mut settings = ExtensionHost::load_settings(paths);
    settings.disabled.retain(|n| n != name);
    if !enabled && !settings.disabled.iter().any(|n| n == name) {
        settings.disabled.push(name.to_string());
    }
    match ExtensionHost::save_settings(paths, &settings) {
        Ok(()) => {
            let mut out = String::new();
            if enabled {
                style::success(&mut out, sty, format!("Extension '{name}' enabled."));
            } else {
                style::info(&mut out, sty, format!("Extension '{name}' disabled."));
            }
            print!("{out}");
            EXIT_SUCCESS
        }
        Err(error) => {
            help::cli_error(format!("save extension settings: {error}"));
            EXIT_ERROR
        }
    }
}
