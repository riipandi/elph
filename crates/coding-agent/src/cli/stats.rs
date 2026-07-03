use clap::Args;

#[derive(Args, Default)]
pub struct StatsArgs {
    /// Filter statistics to a specific session
    #[arg(long, value_name = "SESSION_ID")]
    pub session: Option<String>,

    /// Emit machine-readable JSON output
    #[arg(long)]
    pub json: bool,
}
