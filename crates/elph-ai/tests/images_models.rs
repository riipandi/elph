mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::fake_auth_context;
use elph_ai::auth::{ApiKeyAuth, AuthResolveInput, AuthResult, ModelAuth, ProviderAuth, env_api_key_auth};
use elph_ai::images::{
    CreateImagesProviderOptions, builtin_images_models, create_images_models, create_images_provider,
};
use elph_ai::types::{
    AssistantImages, ContentBlock, ImagesContext, ImagesModel, ImagesOptions, ModelCost, ProviderImages, StopReason,
};
fn sample_context() -> ImagesContext {
    ImagesContext {
        input: vec![ContentBlock::Text {
            text: "a red circle".to_string(),
        }],
    }
}

fn test_image_model(provider: &str, id: &str) -> ImagesModel {
    ImagesModel {
        id: id.to_string(),
        name: id.to_string(),
        api: "test-images".to_string(),
        provider: provider.to_string(),
        base_url: "https://example.test/v1".to_string(),
        input: vec!["text".to_string()],
        output: vec!["image".to_string()],
        cost: ModelCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
        },
        headers: None,
    }
}

fn ok_result(model: &ImagesModel) -> AssistantImages {
    AssistantImages {
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        output: vec![ContentBlock::Image {
            data: "aGk=".to_string(),
            mime_type: "image/png".to_string(),
        }],
        response_id: None,
        usage: None,
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0,
    }
}

struct GenerateCall {
    options: Option<ImagesOptions>,
}

struct TestImagesApi {
    calls: Arc<Mutex<Vec<GenerateCall>>>,
}

impl ProviderImages for TestImagesApi {
    fn generate_images(
        &self,
        model: &ImagesModel,
        _context: &ImagesContext,
        options: Option<ImagesOptions>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AssistantImages> + Send>> {
        let model = model.clone();
        let calls = self.calls.clone();
        Box::pin(async move {
            calls.lock().expect("lock").push(GenerateCall { options });
            ok_result(&model)
        })
    }
}

fn test_provider(
    id: &str,
    models: Vec<ImagesModel>,
    env_var: Option<String>,
    calls: Arc<Mutex<Vec<GenerateCall>>>,
) -> elph_ai::images::ImagesProvider {
    create_images_provider(CreateImagesProviderOptions {
        id: id.to_string(),
        name: None,
        auth: ProviderAuth {
            api_key: env_var.map(|name| match name.as_str() {
                "TEST_KEY" => env_api_key_auth("Test key", vec!["TEST_KEY"]),
                other => panic!("unsupported env var: {other}"),
            }),
            oauth: None,
        },
        models,
        api: Arc::new(TestImagesApi { calls }),
    })
}

#[test]
fn registers_providers_and_reads_models_synchronously() {
    let mut models = create_images_models(None);
    models.set_provider(test_provider(
        "p1",
        vec![test_image_model("p1", "m1"), test_image_model("p1", "m2")],
        None,
        Arc::new(Mutex::new(Vec::new())),
    ));
    models.set_provider(test_provider(
        "p2",
        vec![test_image_model("p2", "m3")],
        None,
        Arc::new(Mutex::new(Vec::new())),
    ));

    let mut provider_ids: Vec<_> = models.get_providers().iter().map(|p| p.id.as_str()).collect();
    provider_ids.sort_unstable();
    assert_eq!(provider_ids, vec!["p1", "p2"]);
    let all_models = models.get_models(None);
    let mut all_model_ids: Vec<_> = all_models.iter().map(|m| m.id.as_str()).collect();
    all_model_ids.sort_unstable();
    assert_eq!(all_model_ids, vec!["m1", "m2", "m3"]);
    let p1_models = models.get_models(Some("p1"));
    assert_eq!(
        p1_models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        vec!["m1", "m2"]
    );
    assert_eq!(models.get_model("p2", "m3").expect("model").id, "m3");
    assert!(models.get_model("p2", "missing").is_none());
}

#[tokio::test]
async fn resolves_auth_and_merges_it_into_requests_explicit_options_win() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let models = create_images_models(Some(elph_ai::images::CreateImagesModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(
            HashMap::from([("TEST_KEY".to_string(), "env-key".to_string())]),
            vec![],
        )),
    }));
    let mut models = models;
    models.set_provider(test_provider(
        "p1",
        vec![test_image_model("p1", "model-a")],
        Some("TEST_KEY".to_string()),
        calls.clone(),
    ));
    let model = models.get_model("p1", "model-a").expect("model");

    let auth = models.get_auth(&model).await.expect("auth").expect("resolved");
    assert_eq!(auth.auth.api_key.as_deref(), Some("env-key"));

    let result = models.generate_images(&model, &sample_context(), None).await;
    assert_eq!(result.stop_reason, StopReason::Stop);
    assert_eq!(
        calls.lock().expect("lock")[0]
            .options
            .as_ref()
            .and_then(|o| o.api_key.as_deref()),
        Some("env-key")
    );

    models
        .generate_images(
            &model,
            &sample_context(),
            Some(ImagesOptions {
                api_key: Some("explicit".to_string()),
                signal: None,
                env: None,
                headers: None,
                timeout_ms: None,
                max_retries: None,
                on_payload: None,
                on_response: None,
            }),
        )
        .await;
    assert_eq!(
        calls.lock().expect("lock")[1]
            .options
            .as_ref()
            .and_then(|o| o.api_key.as_deref()),
        Some("explicit")
    );
}

#[tokio::test]
async fn merges_provider_resolved_env_into_image_options() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut models = create_images_models(None);
    models.set_provider(create_images_provider(CreateImagesProviderOptions {
        id: "p1".to_string(),
        name: None,
        auth: ProviderAuth {
            api_key: Some(ApiKeyAuth {
                name: "Test key".to_string(),
                resolve: Arc::new(|_: AuthResolveInput| {
                    Box::pin(async {
                        Some(AuthResult {
                            auth: ModelAuth {
                                api_key: Some("provider-key".to_string()),
                                base_url: None,
                                headers: None,
                            },
                            env: Some(HashMap::from([
                                ("PROVIDER_ONLY".to_string(), "provider".to_string()),
                                ("SHARED".to_string(), "provider".to_string()),
                            ])),
                            source: None,
                        })
                    })
                }),
                login: None,
            }),
            oauth: None,
        },
        models: vec![test_image_model("p1", "model-a")],
        api: Arc::new(TestImagesApi { calls: calls.clone() }),
    }));
    let model = models.get_model("p1", "model-a").expect("model");

    models
        .generate_images(
            &model,
            &sample_context(),
            Some(ImagesOptions {
                api_key: Some("request-key".to_string()),
                env: Some(HashMap::from([
                    ("REQUEST_ONLY".to_string(), "request".to_string()),
                    ("SHARED".to_string(), "request".to_string()),
                ])),
                signal: None,
                headers: None,
                timeout_ms: None,
                max_retries: None,
                on_payload: None,
                on_response: None,
            }),
        )
        .await;

    let calls_guard = calls.lock().expect("lock");
    let options = calls_guard[0].options.as_ref().expect("options");
    assert_eq!(options.api_key.as_deref(), Some("request-key"));
    let env = options.env.as_ref().expect("env");
    assert_eq!(env.get("PROVIDER_ONLY").map(String::as_str), Some("provider"));
    assert_eq!(env.get("REQUEST_ONLY").map(String::as_str), Some("request"));
    assert_eq!(env.get("SHARED").map(String::as_str), Some("request"));
}

#[tokio::test]
async fn returns_error_result_for_unknown_providers() {
    let models = create_images_models(Some(elph_ai::images::CreateImagesModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(HashMap::new(), vec![])),
    }));
    let ghost = models
        .generate_images(&test_image_model("ghost", "m"), &sample_context(), None)
        .await;
    assert_eq!(ghost.stop_reason, StopReason::Error);
    assert!(
        ghost
            .error_message
            .as_deref()
            .unwrap_or("")
            .contains("Unknown provider: ghost")
    );
}

#[tokio::test]
async fn builtin_images_models_registers_openrouter_provider_with_catalog() {
    let models = builtin_images_models(Some(elph_ai::images::CreateImagesModelsOptions {
        credentials: None,
        auth_context: Some(fake_auth_context(
            HashMap::from([("OPENROUTER_API_KEY".to_string(), "or-key".to_string())]),
            vec![],
        )),
    }));
    let providers: Vec<_> = models.get_providers().iter().map(|p| p.id.as_str()).collect();
    assert_eq!(providers, vec!["openrouter"]);

    let list = models.get_models(Some("openrouter"));
    assert!(!list.is_empty());
    assert!(list.iter().all(|m| m.api == "openrouter-images"));

    let auth = models.get_auth(&list[0]).await.expect("auth").expect("resolved");
    assert_eq!(auth.auth.api_key.as_deref(), Some("or-key"));
}
