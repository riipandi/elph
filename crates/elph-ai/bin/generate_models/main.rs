//! Regenerate embedded model catalogs from [models.dev](https://models.dev) (origin).
//!
//! Usage:
//!   make generate-models
//!   cargo run -p elph-ai --bin generate-models -- chat
//!   cargo run -p elph-ai --bin generate-models -- chat --offline
//!   cargo run -p elph-ai --bin generate-models -- enrich
//!   cargo run -p elph-ai --bin generate-models -- all --no-live-pricing

mod chat;
mod common;
mod image;
mod models_dev;
mod normalize;
mod pricing;
mod provider_sources;
mod term;
mod test_image;
mod thinking_map;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use chat::ChatOptions;
use chat::generate_chat;
use image::ImageOptions;
use image::generate_image;
use test_image::TestImageOptions;
use test_image::generate_test_image;

#[derive(Parser, Debug)]
#[command(
    name = "generate-models",
    about = "Regenerate elph-ai model catalogs from models.dev (origin) + provider pricing APIs"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Rebuild chat catalogs from models.dev + overlays
    Chat(ChatCmd),
    /// Image model catalogs (legacy upstream scripts when available)
    Image(ImageCmd),
    /// Generate tests/data/red-circle.png test fixture
    TestImage(TestImageCmd),
    /// Re-run pricing probes against existing JSON (live → models.dev)
    Enrich(EnrichCmd),
    /// chat + image + test-image
    All(AllCmd),
}

#[derive(Parser, Debug)]
struct ChatCmd {
    #[arg(long)]
    models_dir: Option<PathBuf>,
    /// Use cached models.dev snapshot only
    #[arg(long)]
    offline: bool,
    /// Skip live provider pricing HTTP probes
    #[arg(long)]
    no_live_pricing: bool,
    /// Bypass the models.dev cache freshness check (always re-fetch)
    #[arg(long)]
    force: bool,
}

#[derive(Parser, Debug)]
struct EnrichCmd {
    #[arg(long)]
    models_dir: Option<PathBuf>,
    #[arg(long)]
    offline: bool,
    #[arg(long)]
    no_live_pricing: bool,
    #[arg(long)]
    force: bool,
}

#[derive(Parser, Debug)]
struct ImageCmd {
    #[arg(long)]
    skip_scripts: bool,
    #[arg(long)]
    images_dir: Option<PathBuf>,
    #[arg(long)]
    no_regenerate_catalog: bool,
}

#[derive(Parser, Debug)]
struct TestImageCmd {
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct AllCmd {
    #[arg(long)]
    models_dir: Option<PathBuf>,
    #[arg(long)]
    images_dir: Option<PathBuf>,
    #[arg(long)]
    test_image_output: Option<PathBuf>,
    /// Skip regenerating `src/images/models.rs` (image catalogs only)
    #[arg(long)]
    no_regenerate_catalog: bool,
    #[arg(long)]
    offline: bool,
    #[arg(long)]
    no_live_pricing: bool,
    #[arg(long)]
    skip_scripts: bool,
    #[arg(long)]
    force: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    match args.command {
        Command::Chat(cmd) => generate_chat(ChatOptions {
            models_dir: cmd.models_dir.unwrap_or_else(|| crate_root.join("models")),
            builtin_rs: crate_root.join("src/providers/builtin.rs"),
            offline: cmd.offline,
            no_live_pricing: cmd.no_live_pricing,
            force: cmd.force,
        }),
        Command::Image(cmd) => {
            // Image path still uses optional local pi clone when present.
            let catalog_dir = common::default_catalog_dir(&crate_root);
            generate_image(ImageOptions {
                catalog_dir,
                skip_scripts: cmd.skip_scripts,
                images_dir: cmd.images_dir.unwrap_or_else(|| crate_root.join("models/images")),
                models_rs: crate_root.join("src/images/models.rs"),
                no_regenerate_catalog: cmd.no_regenerate_catalog,
            })
        }
        Command::TestImage(cmd) => generate_test_image(TestImageOptions {
            output: cmd
                .output
                .unwrap_or_else(|| crate_root.join("tests/data/red-circle.png")),
        }),
        Command::Enrich(cmd) => {
            // Re-run full chat rebuild (includes pricing); dedicated enrich keeps same entry.
            generate_chat(ChatOptions {
                models_dir: cmd.models_dir.unwrap_or_else(|| crate_root.join("models")),
                builtin_rs: crate_root.join("src/providers/builtin.rs"),
                offline: cmd.offline,
                no_live_pricing: cmd.no_live_pricing,
                force: cmd.force,
            })
        }
        Command::All(cmd) => {
            generate_chat(ChatOptions {
                models_dir: cmd.models_dir.clone().unwrap_or_else(|| crate_root.join("models")),
                builtin_rs: crate_root.join("src/providers/builtin.rs"),
                offline: cmd.offline,
                no_live_pricing: cmd.no_live_pricing,
                force: cmd.force,
            })?;
            let catalog_dir = common::default_catalog_dir(&crate_root);
            let _ = generate_image(ImageOptions {
                catalog_dir,
                skip_scripts: cmd.skip_scripts,
                images_dir: cmd.images_dir.unwrap_or_else(|| crate_root.join("models/images")),
                models_rs: crate_root.join("src/images/models.rs"),
                no_regenerate_catalog: cmd.no_regenerate_catalog,
            });
            generate_test_image(TestImageOptions {
                output: cmd
                    .test_image_output
                    .unwrap_or_else(|| crate_root.join("tests/data/red-circle.png")),
            })?;
            Ok(())
        }
    }
}
