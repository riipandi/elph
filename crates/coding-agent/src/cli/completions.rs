use clap::Args;

#[derive(Args, Default)]
pub struct CompletionsArgs {
    /// Shell type for completion script
    #[arg(short, long, value_name = "SHELL", default_value = "bash")]
    pub shell: String,
}
