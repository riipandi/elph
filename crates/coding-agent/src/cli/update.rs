use clap::Args;

#[derive(Args)]
pub struct UpdateArgs {
    /// Check for updates without installing
    #[arg(long)]
    pub check: bool,

    /// Emit machine-readable JSON output (for --check)
    #[arg(long)]
    pub json: bool,

    /// Force re-download and install even if already up to date
    #[arg(long)]
    pub force_reinstall: bool,

    /// Install a specific version (e.g. 0.0.0 or 0.0.0-canary)
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,

    /// Switch to the canary release channel (faster updates, may have bugs)
    #[arg(long)]
    pub canary: bool,

    /// Switch to the stable release channel (default, weekly releases)
    #[arg(long)]
    pub stable: bool,
}
