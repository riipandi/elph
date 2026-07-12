//! Connector configuration auth. Excludes slack, gmail, notion, and OAuth flows.

mod configure;
mod providers;

pub use configure::{ConfigureResult, configure_connector};
pub use providers::{format_auth_provider_list, is_configure_connector, is_unsupported_auth_provider};

use anyhow::{Result, bail};

pub fn run_auth_configure(provider: &str, force: bool) -> Result<ConfigureResult> {
    if is_unsupported_auth_provider(provider) && provider != "x" {
        bail!("OpenWiki auth configure for `{provider}` is not supported in Owly.");
    }
    let connector = provider_to_connector(provider)?;
    configure_connector(connector, force)
}

pub fn run_auth_list() -> Result<()> {
    println!("{}", format_auth_provider_list());
    Ok(())
}

fn provider_to_connector(provider: &str) -> Result<crate::connectors::ConnectorId> {
    match provider {
        "x" => Ok(crate::connectors::ConnectorId::X),
        "git-repo" | "web-search" | "hackernews" => {
            let id = crate::connectors::ConnectorId::parse(provider).unwrap();
            Ok(id)
        }
        other if crate::connectors::ConnectorId::parse(other).is_some() => {
            Ok(crate::connectors::ConnectorId::parse(other).unwrap())
        }
        _ => bail!("Unknown provider for configure: {provider}"),
    }
}
