//! Regenerate embedded model catalogs from [@earendil-works/pi-ai](https://github.com/earendil-works/pi/tree/main/packages/ai).
//!
//! Catalog source path is fixed: `../../earendil-works/pi/packages/ai` relative to the elph
//! workspace root (see [`common::default_catalog_dir`]). No `--catalog-dir` flag.
//!
//! Usage:
//!   make generate-models
//!   make generate-models ARGS="--skip-scripts"
//!   cargo run -p elph-ai --bin generate-models -- chat
//!   cargo run -p elph-ai --bin generate-models -- image
//!   cargo run -p elph-ai --bin generate-models -- test-image
//!   cargo run -p elph-ai --bin generate-models -- all
//!   cargo run -p elph-ai --bin generate-models -- all --skip-scripts

mod chat;
mod common;
mod image;
mod test_image;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use chat::ChatOptions;
use chat::generate_chat;
use common::default_catalog_dir;
use image::ImageOptions;
use image::generate_image;
use test_image::TestImageOptions;
use test_image::generate_test_image;

#[derive(Parser, Debug)]
#[command(
    name = "generate-models",
    about = "Regenerate elph-ai model catalogs from the fixed catalog source (earendil-works/pi/packages/ai)"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Regenerate chat model catalogs from catalog npm scripts
    Chat(ChatCmd),
    /// Regenerate image model catalogs from catalog npm scripts
    Image(ImageCmd),
    /// Generate tests/data/red-circle.png test fixture
    TestImage(TestImageCmd),
    /// Run chat, image, and test-image
    All(AllCmd),
}

#[derive(Parser, Debug)]
struct SkipScripts {
    /// Skip running catalog source npm scripts and only convert existing generated files
    #[arg(long)]
    skip_scripts: bool,
}

#[derive(Parser, Debug)]
struct ChatCmd {
    #[command(flatten)]
    skip: SkipScripts,

    /// Output directory for JSON catalogs (default: crates/elph-ai/models)
    #[arg(long)]
    models_dir: Option<PathBuf>,

    /// Only write JSON catalogs; skip regenerating src/models/catalog.rs
    #[arg(long)]
    no_regenerate_catalog: bool,
}

#[derive(Parser, Debug)]
struct ImageCmd {
    #[command(flatten)]
    skip: SkipScripts,

    /// Output directory for image JSON catalogs (default: crates/elph-ai/models/images)
    #[arg(long)]
    images_dir: Option<PathBuf>,

    /// Only write JSON catalogs; skip regenerating src/images/models.rs
    #[arg(long)]
    no_regenerate_catalog: bool,
}

#[derive(Parser, Debug)]
struct TestImageCmd {
    /// Output path (default: crates/elph-ai/tests/data/red-circle.png)
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Parser, Debug)]
struct AllCmd {
    #[command(flatten)]
    skip: SkipScripts,

    #[arg(long)]
    models_dir: Option<PathBuf>,

    #[arg(long)]
    images_dir: Option<PathBuf>,

    #[arg(long)]
    test_image_output: Option<PathBuf>,

    #[arg(long)]
    no_regenerate_catalog: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let catalog_dir = default_catalog_dir(&crate_root);

    match args.command {
        Command::Chat(cmd) => generate_chat(ChatOptions {
            catalog_dir,
            skip_scripts: cmd.skip.skip_scripts,
            models_dir: cmd.models_dir.unwrap_or_else(|| crate_root.join("models")),
            catalog_rs: crate_root.join("src/models/catalog.rs"),
            no_regenerate_catalog: cmd.no_regenerate_catalog,
        }),
        Command::Image(cmd) => generate_image(ImageOptions {
            catalog_dir,
            skip_scripts: cmd.skip.skip_scripts,
            images_dir: cmd.images_dir.unwrap_or_else(|| crate_root.join("models/images")),
            models_rs: crate_root.join("src/images/models.rs"),
            no_regenerate_catalog: cmd.no_regenerate_catalog,
        }),
        Command::TestImage(cmd) => generate_test_image(TestImageOptions {
            output: cmd
                .output
                .unwrap_or_else(|| crate_root.join("tests/data/red-circle.png")),
        }),
        Command::All(cmd) => {
            generate_chat(ChatOptions {
                catalog_dir: catalog_dir.clone(),
                skip_scripts: cmd.skip.skip_scripts,
                models_dir: cmd.models_dir.unwrap_or_else(|| crate_root.join("models")),
                catalog_rs: crate_root.join("src/models/catalog.rs"),
                no_regenerate_catalog: cmd.no_regenerate_catalog,
            })?;
            generate_image(ImageOptions {
                catalog_dir,
                skip_scripts: cmd.skip.skip_scripts,
                images_dir: cmd.images_dir.unwrap_or_else(|| crate_root.join("models/images")),
                models_rs: crate_root.join("src/images/models.rs"),
                no_regenerate_catalog: cmd.no_regenerate_catalog,
            })?;
            generate_test_image(TestImageOptions {
                output: cmd
                    .test_image_output
                    .unwrap_or_else(|| crate_root.join("tests/data/red-circle.png")),
            })
        }
    }
}
