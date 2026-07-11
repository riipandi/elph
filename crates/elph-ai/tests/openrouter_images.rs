use std::sync::{Arc, Mutex};

use axum::{Json, Router, routing::post};
use elph_ai::api::OpenRouterImagesApi;
use elph_ai::types::{ContentBlock, ImagesContext, ImagesModel, ImagesOptions, ModelCost, ProviderImages, StopReason};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

fn mock_chat_completion_response() -> Value {
    json!({
        "id": "img-1",
        "usage": {
            "prompt_tokens": 12,
            "completion_tokens": 34,
            "prompt_tokens_details": { "cached_tokens": 0 }
        },
        "choices": [{
            "message": {
                "content": "Here is your image.",
                "images": [{ "image_url": "data:image/png;base64,ZmFrZS1wbmc=" }]
            }
        }]
    })
}

async fn start_mock_server(response: Value) -> (String, Arc<Mutex<Option<Value>>>, tokio::task::JoinHandle<()>) {
    let captured = Arc::new(Mutex::new(None));
    let captured_for_handler = captured.clone();
    let app = Router::new().route(
        "/chat/completions",
        post(move |Json(body): Json<Value>| {
            let captured = captured_for_handler.clone();
            let response = response.clone();
            async move {
                *captured.lock().expect("lock") = Some(body);
                Json(response)
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    (base_url, captured, server)
}

fn text_and_image_model(base_url: &str) -> ImagesModel {
    ImagesModel {
        id: "google/gemini-3.1-flash-image-preview".to_string(),
        name: "Gemini 3.1 Flash Image Preview".to_string(),
        api: "openrouter-images".to_string(),
        provider: "openrouter".to_string(),
        base_url: base_url.to_string(),
        input: vec!["text".to_string(), "image".to_string()],
        output: vec!["text".to_string(), "image".to_string()],
        cost: ModelCost {
            input: 0.015,
            output: 0.03,
            cache_read: 0.0,
            cache_write: 0.0,

            tiers: None,
        },
        headers: Some([("HTTP-Referer".to_string(), "https://example.com".to_string())].into()),
    }
}

fn image_only_model(base_url: &str) -> ImagesModel {
    ImagesModel {
        id: "black-forest-labs/flux.2-pro".to_string(),
        name: "FLUX.2 Pro".to_string(),
        api: "openrouter-images".to_string(),
        provider: "openrouter".to_string(),
        base_url: base_url.to_string(),
        input: vec!["text".to_string(), "image".to_string()],
        output: vec!["image".to_string()],
        cost: ModelCost {
            input: 0.015,
            output: 0.03,
            cache_read: 0.0,
            cache_write: 0.0,

            tiers: None,
        },
        headers: None,
    }
}

#[tokio::test]
async fn returns_text_plus_images_in_final_output() {
    let (base_url, captured, server) = start_mock_server(mock_chat_completion_response()).await;
    let model = text_and_image_model(&base_url);
    let context = ImagesContext {
        input: vec![ContentBlock::Text {
            text: "Generate a dog".to_string(),
        }],
    };
    let output = OpenRouterImagesApi
        .generate_images(
            &model,
            &context,
            Some(ImagesOptions {
                api_key: Some("test".to_string()),
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

    server.abort();

    assert_eq!(output.stop_reason, StopReason::Stop);
    assert_eq!(output.response_id.as_deref(), Some("img-1"));
    assert!(matches!(output.output[0], ContentBlock::Text { .. }));
    if let ContentBlock::Text { text } = &output.output[0] {
        assert_eq!(text, "Here is your image.");
    }
    if let ContentBlock::Image { mime_type, data } = &output.output[1] {
        assert_eq!(mime_type, "image/png");
        assert_eq!(data, "ZmFrZS1wbmc=");
    } else {
        panic!("expected image block");
    }

    let params = captured.lock().expect("lock").clone().expect("params");
    assert_eq!(params["stream"], false);
    assert_eq!(params["modalities"], json!(["image", "text"]));
    assert_eq!(
        params["messages"][0]["content"][0],
        json!({ "type": "text", "text": "Generate a dog" })
    );
}

#[tokio::test]
async fn generate_images_resolves_final_assistant_images_result() {
    let (base_url, _captured, server) = start_mock_server(mock_chat_completion_response()).await;
    let model = image_only_model(&base_url);
    let context = ImagesContext {
        input: vec![ContentBlock::Text {
            text: "Generate a dog".to_string(),
        }],
    };

    let output = OpenRouterImagesApi
        .generate_images(
            &model,
            &context,
            Some(ImagesOptions {
                api_key: Some("test".to_string()),
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

    server.abort();

    assert!(
        output
            .output
            .iter()
            .any(|item| matches!(item, ContentBlock::Image { .. }))
    );
}

#[tokio::test]
async fn passes_through_abort_signal_and_returns_aborted_result() {
    let token = CancellationToken::new();
    token.cancel();
    let model = image_only_model("http://127.0.0.1:9");
    let context = ImagesContext {
        input: vec![ContentBlock::Text {
            text: "Generate a dog".to_string(),
        }],
    };

    let output = OpenRouterImagesApi
        .generate_images(
            &model,
            &context,
            Some(ImagesOptions {
                api_key: Some("test".to_string()),
                signal: Some(token),
                env: None,
                headers: None,
                timeout_ms: None,
                max_retries: None,
                on_payload: None,
                on_response: None,
            }),
        )
        .await;

    assert_eq!(output.stop_reason, StopReason::Aborted);
    assert_eq!(output.error_message.as_deref(), Some("Request aborted"));
}
