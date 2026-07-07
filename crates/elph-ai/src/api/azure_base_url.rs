use anyhow::{Result, anyhow};

pub fn build_default_azure_base_url(resource_name: &str) -> String {
    format!("https://{resource_name}.openai.azure.com/openai/v1")
}

pub fn normalize_azure_base_url(base_url: &str) -> Result<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    let mut url = url::Url::parse(trimmed).map_err(|_| anyhow!("Invalid Azure OpenAI base URL: {base_url}"))?;
    let host = url.host_str().unwrap_or("").to_lowercase();
    let is_azure = host.ends_with(".openai.azure.com")
        || host.ends_with(".cognitiveservices.azure.com")
        || host.ends_with(".ai.azure.com");
    let path = url.path().trim_end_matches('/');
    if is_azure && (path.is_empty() || path == "/" || path == "/openai" || path == "/openai/v1/responses") {
        url.set_path("/openai/v1");
        url.set_query(None);
    }
    Ok(url.to_string().trim_end_matches('/').to_string())
}
