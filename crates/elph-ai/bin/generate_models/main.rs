//! Regenerate embedded model catalogs from [@earendil-works/pi-ai](https://github.com/earendil-works/pi/tree/main/packages/ai).
//!
//! Usage:
//!   make generate-models
//!   make generate-models PI_AI_DIR=/path/to/pi/packages/ai ARGS="--skip-pi"
//!   cargo run -p elph-ai --bin generate-models -- chat --pi-dir /path/to/pi/packages/ai
//!   cargo run -p elph-ai --bin generate-models -- image --pi-dir /path/to/pi/packages/ai
//!   cargo run -p elph-ai --bin generate-models -- test-image
//!   cargo run -p elph-ai --bin generate-models -- all --pi-dir /path/to/pi/packages/ai

mod chat;
mod common;
mod image;
mod test_image;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use chat::{ChatOptions, generate_chat};
use image::{ImageOptions, generate_image};
use test_image::{TestImageOptions, generate_test_image};

#[derive(Parser, Debug)]
#[command(
    name = "generate-models",
    about = "Regenerate elph-ai model catalogs from pi-ai scripts"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Regenerate chat model catalogs from pi-ai generate-models.ts
    Chat(ChatCmd),
    /// Regenerate image model catalogs from pi-ai generate-image-models.ts
    Image(ImageCmd),
    /// Generate tests/data/red-circle.png (pi-ai generate-test-image.ts equivalent)
    TestImage(TestImageCmd),
    /// Run chat, image, and test-image
    All(AllCmd),
}

#[derive(Parser, Debug)]
struct PiCommon {
    /// Path to pi-ai package root (packages/ai), e.g. ../pi/packages/ai
    #[arg(long, env = "PI_AI_DIR")]
    pi_dir: PathBuf,

    /// Skip running pi-ai npm scripts and only convert existing generated files
    #[arg(long)]
    skip_pi: bool,
}

#[derive(Parser, Debug)]
struct ChatCmd {
    #[command(flatten)]
    pi: PiCommon,

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
    pi: PiCommon,

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
    pi: PiCommon,

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

    match args.command {
        Command::Chat(cmd) => generate_chat(ChatOptions {
            pi_dir: cmd.pi.pi_dir,
            skip_pi: cmd.pi.skip_pi,
            models_dir: cmd.models_dir.unwrap_or_else(|| crate_root.join("models")),
            catalog_rs: crate_root.join("src/models/catalog.rs"),
            no_regenerate_catalog: cmd.no_regenerate_catalog,
        }),
        Command::Image(cmd) => generate_image(ImageOptions {
            pi_dir: cmd.pi.pi_dir,
            skip_pi: cmd.pi.skip_pi,
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
                pi_dir: cmd.pi.pi_dir.clone(),
                skip_pi: cmd.pi.skip_pi,
                models_dir: cmd.models_dir.unwrap_or_else(|| crate_root.join("models")),
                catalog_rs: crate_root.join("src/models/catalog.rs"),
                no_regenerate_catalog: cmd.no_regenerate_catalog,
            })?;
            generate_image(ImageOptions {
                pi_dir: cmd.pi.pi_dir,
                skip_pi: cmd.pi.skip_pi,
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
