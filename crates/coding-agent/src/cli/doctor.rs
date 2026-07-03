use clap::Args;

#[derive(Args, Default)]
pub struct DoctorArgs {
    /// Emit machine-readable JSON output
    #[arg(long)]
    pub json: bool,
}
