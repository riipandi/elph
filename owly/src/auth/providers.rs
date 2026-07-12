const UNSUPPORTED: &[&str] = &["slack", "gmail", "google", "notion", "x"];

const CONFIGURE_CONNECTORS: &[&str] = &["git-repo", "web-search", "hackernews"];

pub fn is_unsupported_auth_provider(value: &str) -> bool {
    UNSUPPORTED.contains(&value)
}

pub fn is_configure_connector(value: &str) -> bool {
    CONFIGURE_CONNECTORS.contains(&value)
}

pub fn format_auth_provider_list() -> String {
    [
        "Connector auth in Owly uses `owly auth configure <connector>` (no OAuth CLI).",
        "",
        "Supported connectors:",
        "  git-repo     Local git repositories",
        "  web-search   Web search (set TAVILY_API_KEY)",
        "  hackernews   Hacker News feeds",
        "",
        "Optional: `owly auth configure x` then set OWLY_X_ACCESS_TOKEN in ~/.owly/.env.",
        "",
        "Not supported: slack, gmail, notion, auth tools, OAuth flows (including x).",
    ]
    .join("\n")
}
