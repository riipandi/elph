use elph_ai::api::azure_base_url::{build_default_azure_base_url, normalize_azure_base_url};

#[test]
fn normalizes_cognitive_services_root_endpoints_to_openai_v1() {
    let normalized =
        normalize_azure_base_url("https://marc-quicktests-resource.cognitiveservices.azure.com").expect("url");
    assert_eq!(
        normalized,
        "https://marc-quicktests-resource.cognitiveservices.azure.com/openai/v1"
    );
}

#[test]
fn normalizes_microsoft_foundry_root_endpoints_to_openai_v1() {
    let normalized = normalize_azure_base_url("https://marc-quicktests-resource.ai.azure.com").expect("url");
    assert_eq!(normalized, "https://marc-quicktests-resource.ai.azure.com/openai/v1");
}

#[test]
fn normalizes_azure_openai_root_endpoints_to_openai_v1() {
    let normalized = normalize_azure_base_url("https://my-resource.openai.azure.com").expect("url");
    assert_eq!(normalized, "https://my-resource.openai.azure.com/openai/v1");
}

#[test]
fn normalizes_openai_path_to_openai_v1() {
    let normalized = normalize_azure_base_url("https://my-resource.cognitiveservices.azure.com/openai").expect("url");
    assert_eq!(normalized, "https://my-resource.cognitiveservices.azure.com/openai/v1");
}

#[test]
fn preserves_openai_v1_endpoints() {
    let normalized =
        normalize_azure_base_url("https://my-resource.cognitiveservices.azure.com/openai/v1").expect("url");
    assert_eq!(normalized, "https://my-resource.cognitiveservices.azure.com/openai/v1");
}

#[test]
fn normalizes_openai_v1_responses_to_openai_v1() {
    let normalized =
        normalize_azure_base_url("https://my-resource.services.ai.azure.com/openai/v1/responses").expect("url");
    assert_eq!(normalized, "https://my-resource.services.ai.azure.com/openai/v1");
}

#[test]
fn preserves_explicit_non_azure_proxy_paths() {
    let normalized = normalize_azure_base_url("https://my-proxy.example.com/v1").expect("url");
    assert_eq!(normalized, "https://my-proxy.example.com/v1");
}

#[test]
fn strips_query_params_when_normalizing_azure_host_urls() {
    let normalized =
        normalize_azure_base_url("https://my-resource.openai.azure.com/openai?api-version=2024-12-01").expect("url");
    assert_eq!(normalized, "https://my-resource.openai.azure.com/openai/v1");
}

#[test]
fn preserves_query_params_on_non_azure_proxy_urls() {
    let normalized = normalize_azure_base_url("https://my-proxy.example.com/v1?custom=true").expect("url");
    assert_eq!(normalized, "https://my-proxy.example.com/v1?custom=true");
}

#[test]
fn rejects_invalid_urls() {
    let err = normalize_azure_base_url("not-a-url").unwrap_err();
    assert!(err.to_string().contains("Invalid Azure OpenAI base URL"));
}

#[test]
fn builds_default_url_from_resource_name() {
    assert_eq!(
        build_default_azure_base_url("my-resource"),
        "https://my-resource.openai.azure.com/openai/v1"
    );
}
