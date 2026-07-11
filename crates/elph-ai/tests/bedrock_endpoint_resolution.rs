use elph_ai::api::bedrock_shared::{
    BedrockThinkingOptions, get_standard_bedrock_endpoint_region, resolve_bedrock_runtime_config,
};
use elph_ai::get_builtin_model;

fn thinking_options<'a>(
    region: Option<&'a str>,
    profile: Option<&'a str>,
    ambient_profile: Option<&'a str>,
    env: Option<&'a std::collections::HashMap<String, String>>,
) -> BedrockThinkingOptions<'a> {
    BedrockThinkingOptions {
        region,
        profile,
        ambient_profile,
        reasoning: None,
        thinking_budgets: None,
        thinking_display: None,
        interleaved_thinking: true,
        env,
    }
}

#[test]
fn assigns_eu_central_1_runtime_urls_to_builtin_eu_inference_profiles() {
    let model = get_builtin_model("amazon-bedrock", "eu.anthropic.claude-sonnet-4-5-20250929-v1:0").expect("model");
    assert_eq!(model.base_url, "https://bedrock-runtime.eu-central-1.amazonaws.com");
}

#[test]
fn does_not_pin_standard_endpoints_when_aws_region_is_configured() {
    let model = get_builtin_model("amazon-bedrock", "us.anthropic.claude-opus-4-8").expect("model");
    let env = std::collections::HashMap::from([("AWS_REGION".to_string(), "us-east-2".to_string())]);
    let config = resolve_bedrock_runtime_config(&model, &thinking_options(None, None, None, Some(&env)));
    assert_eq!(config.region.as_deref(), Some("us-east-2"));
    assert!(config.endpoint.is_none());
}

#[test]
fn derives_region_from_builtin_eu_endpoint_when_no_region_or_profile_is_configured() {
    let model = get_builtin_model("amazon-bedrock", "eu.anthropic.claude-sonnet-4-5-20250929-v1:0").expect("model");
    let config = resolve_bedrock_runtime_config(&model, &thinking_options(None, None, None, None));
    assert_eq!(
        config.endpoint.as_deref(),
        Some("https://bedrock-runtime.eu-central-1.amazonaws.com")
    );
    assert_eq!(config.region.as_deref(), Some("eu-central-1"));
}

#[test]
fn handles_missing_regions_for_explicit_scoped_and_ambient_profiles() {
    let model = get_builtin_model("amazon-bedrock", "eu.anthropic.claude-sonnet-4-5-20250929-v1:0").expect("model");

    let explicit = resolve_bedrock_runtime_config(&model, &thinking_options(None, Some("bedrock-profile"), None, None));
    assert_eq!(explicit.profile.as_deref(), Some("bedrock-profile"));
    assert_eq!(
        explicit.endpoint.as_deref(),
        Some("https://bedrock-runtime.eu-central-1.amazonaws.com")
    );
    assert_eq!(explicit.region.as_deref(), Some("eu-central-1"));

    let scoped_env =
        std::collections::HashMap::from([("AWS_PROFILE".to_string(), "scoped-bedrock-profile".to_string())]);
    let scoped = resolve_bedrock_runtime_config(&model, &thinking_options(None, None, None, Some(&scoped_env)));
    assert_eq!(scoped.profile.as_deref(), Some("scoped-bedrock-profile"));
    assert_eq!(
        scoped.endpoint.as_deref(),
        Some("https://bedrock-runtime.eu-central-1.amazonaws.com")
    );
    assert_eq!(scoped.region.as_deref(), Some("eu-central-1"));

    let ambient =
        resolve_bedrock_runtime_config(&model, &thinking_options(None, None, Some("ambient-bedrock-profile"), None));
    assert_eq!(ambient.profile.as_deref(), Some("ambient-bedrock-profile"));
    assert!(ambient.endpoint.is_none());
    assert!(ambient.region.is_none());
}

#[test]
fn passes_custom_bedrock_endpoints_through() {
    let mut model = get_builtin_model("amazon-bedrock", "us.anthropic.claude-opus-4-8").expect("model");
    model.base_url = "https://bedrock-vpc.example.com".to_string();
    let env = std::collections::HashMap::from([("AWS_REGION".to_string(), "us-west-2".to_string())]);
    let config = resolve_bedrock_runtime_config(&model, &thinking_options(None, None, None, Some(&env)));
    assert_eq!(config.endpoint.as_deref(), Some("https://bedrock-vpc.example.com"));
    assert_eq!(config.region.as_deref(), Some("us-west-2"));
}

#[test]
fn extracts_region_from_inference_profile_arn() {
    let mut model = get_builtin_model("amazon-bedrock", "us.anthropic.claude-opus-4-8").expect("model");
    model.id = "arn:aws:bedrock:us-west-2:123456789012:application-inference-profile/abc123".to_string();
    let env = std::collections::HashMap::from([("AWS_REGION".to_string(), "us-east-1".to_string())]);
    let config = resolve_bedrock_runtime_config(&model, &thinking_options(None, None, None, Some(&env)));
    assert_eq!(config.region.as_deref(), Some("us-west-2"));
}

#[test]
fn extracts_region_from_govcloud_inference_profile_arn() {
    let mut model = get_builtin_model("amazon-bedrock", "us.anthropic.claude-opus-4-8").expect("model");
    model.id = "arn:aws-us-gov:bedrock:us-gov-west-1:123456789012:application-inference-profile/abc123".to_string();
    let env = std::collections::HashMap::from([("AWS_REGION".to_string(), "us-east-1".to_string())]);
    let config = resolve_bedrock_runtime_config(&model, &thinking_options(None, None, None, Some(&env)));
    assert_eq!(config.region.as_deref(), Some("us-gov-west-1"));
}

#[test]
fn parses_standard_bedrock_runtime_host_regions() {
    assert_eq!(
        get_standard_bedrock_endpoint_region("https://bedrock-runtime.eu-central-1.amazonaws.com"),
        Some("eu-central-1".to_string())
    );
}
