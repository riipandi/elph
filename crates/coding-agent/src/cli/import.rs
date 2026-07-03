use clap::Args;

#[derive(Args)]
pub struct ImportArgs {
    /// Path to session file, directory, or share URL
    #[arg(value_name = "FILE")]
    pub file: Option<String>,

    /// List available sessions without importing
    #[arg(long)]
    pub list: bool,

    /// Emit NDJSON output to stdout
    #[arg(long)]
    pub json: bool,
}
