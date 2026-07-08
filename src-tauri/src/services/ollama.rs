use std::time::Duration;

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, OllamaStatus};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
/// Local chat models can take a while to generate structured taxonomy/assignment output.
const CHAT_TIMEOUT: Duration = Duration::from_secs(180);

/// Thin HTTP client for a local (or LAN) Ollama server. Base URL is never hardcoded —
/// it always comes from settings so Ollama can run on a different machine.
#[derive(Debug, Clone)]
pub struct OllamaClient {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> AppResult<Self> {
        let model = model.into().trim().to_string();
        if model.is_empty() {
            return Err(AppError::ollama("set a chat model in Settings"));
        }
        let client = Client::builder().build()?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model,
        })
    }

    pub fn from_settings(settings: &AppSettings) -> AppResult<Self> {
        Self::new(settings.ollama_base_url.clone(), settings.ollama_chat_model.clone())
    }

    /// Never fails on unreachable/erroring servers — Ollama is optional at runtime, so
    /// callers should degrade gracefully instead of surfacing a hard error.
    pub async fn health_check(&self) -> AppResult<OllamaStatus> {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).timeout(HEALTH_TIMEOUT).send().await {
            Ok(response) if response.status().is_success() => Ok(OllamaStatus {
                available: true,
                message: "Ollama is reachable".to_string(),
            }),
            Ok(response) => Ok(OllamaStatus {
                available: false,
                message: format!("Ollama responded with HTTP {}", response.status()),
            }),
            Err(e) => Ok(OllamaStatus {
                available: false,
                message: format!("Ollama unreachable: {e}"),
            }),
        }
    }

    /// POST /api/chat with a structured-output JSON schema, then parse the assistant's
    /// message content (itself a JSON string) into `T`.
    pub async fn chat_json<T: DeserializeOwned>(
        &self,
        system: &str,
        user: &str,
        format_schema: Value,
    ) -> AppResult<T> {
        let url = format!("{}/api/chat", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "format": format_schema,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });

        let response = self
            .client
            .post(&url)
            .timeout(CHAT_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::ollama(format!("failed to reach Ollama: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::ollama(format!(
                "Ollama chat request failed with HTTP {status}: {text}"
            )));
        }

        let parsed: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| AppError::ollama(format!("invalid response from Ollama: {e}")))?;

        serde_json::from_str::<T>(&parsed.message.content)
            .map_err(|e| AppError::ollama(format!("model returned invalid JSON: {e}")))
    }
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaChatMessage {
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TaxonomyDraft;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn empty_model_is_rejected() {
        let err = OllamaClient::new("http://127.0.0.1:11434", "  ").unwrap_err();
        assert_eq!(err.code, "ollama_error");
    }

    #[test]
    fn trims_trailing_slash_from_base_url() {
        let client = OllamaClient::new("http://127.0.0.1:11434/", "qwen3:14b").expect("client");
        assert_eq!(client.base_url, "http://127.0.0.1:11434");
    }

    #[tokio::test]
    async fn health_check_reports_available_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "models": [] })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = OllamaClient::new(server.uri(), "qwen3:14b").expect("client");
        let status = client.health_check().await.expect("health check");
        assert!(status.available);
    }

    #[tokio::test]
    async fn health_check_reports_unavailable_when_server_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&server)
            .await;

        let client = OllamaClient::new(server.uri(), "qwen3:14b").expect("client");
        let status = client.health_check().await.expect("health check");
        assert!(!status.available);
    }

    #[tokio::test]
    async fn chat_json_parses_structured_content_into_taxonomy_draft() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "role": "assistant",
                    "content": "{\"categories\":[{\"name\":\"AI/LLM\",\"subcategories\":[\"Agent Frameworks\"]}]}"
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = OllamaClient::new(server.uri(), "qwen3:14b").expect("client");
        let draft: TaxonomyDraft = client
            .chat_json("propose a taxonomy", "repo list", serde_json::json!({}))
            .await
            .expect("chat_json");

        assert_eq!(draft.categories.len(), 1);
        assert_eq!(draft.categories[0].name, "AI/LLM");
        assert_eq!(draft.categories[0].subcategories, vec!["Agent Frameworks"]);
    }
}
