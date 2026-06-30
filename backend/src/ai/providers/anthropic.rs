use crate::ai::{AiCapabilities, AiProvider, AiRequest};
use async_trait::async_trait;

const ANTHROPIC_API: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    id: String,
    #[allow(dead_code)]
    name: String,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(id: String, name: String, api_key: String, model: String) -> Self {
        Self { id, name, api_key, model }
    }

    fn messages_url(&self) -> String {
        format!("{}/v1/messages", ANTHROPIC_API)
    }
}

#[async_trait]
impl AiProvider for AnthropicProvider {
    fn id(&self) -> &str { &self.id }
    fn display_name(&self) -> &str { &self.name }

    fn capabilities(&self) -> AiCapabilities {
        AiCapabilities {
            reasoning: 10,
            coding: 10,
            tool_use: true,
            vision: true,
            local: false,
            streaming: true,
        }
    }

    async fn complete(&self, req: &AiRequest) -> std::result::Result<String, String> {
        let (system, messages) = split_messages(req);
        let body = build_body(&self.model, system, messages, false);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build().map_err(|e| e.to_string())?;

        let resp = client
            .post(self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send().await
            .map_err(|e| format!("Anthropic unreachable: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Anthropic error {status}: {text}"));
        }
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        json.get("content")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|b| b.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Unexpected Anthropic response shape".to_string())
    }

    async fn stream(&self, req: &AiRequest) -> std::result::Result<reqwest::Response, String> {
        let (system, messages) = split_messages(req);
        let body = build_body(&self.model, system, messages, true);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build().map_err(|e| e.to_string())?;

        client
            .post(self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send().await
            .map_err(|e| format!("Anthropic unreachable: {e}"))
    }

    async fn health_check(&self) -> std::result::Result<(), String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build().map_err(|e| e.to_string())?;

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 1,
            "messages": [{ "role": "user", "content": "ping" }]
        });

        let resp = client
            .post(self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send().await
            .map_err(|e| e.to_string())?;

        let status = resp.status();
        if status.is_success() || status.as_u16() == 400 {
            Ok(())
        } else {
            Err(format!("HTTP {status}"))
        }
    }
}

fn split_messages(req: &AiRequest) -> (Option<String>, Vec<serde_json::Value>) {
    let system = req.system_prompt.clone();
    let messages = req.messages.iter().map(|m| {
        serde_json::json!({ "role": m.role, "content": m.content })
    }).collect();
    (system, messages)
}

fn build_body(
    model: &str,
    system: Option<String>,
    messages: Vec<serde_json::Value>,
    stream: bool,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "messages": messages,
        "stream": stream,
    });
    if let Some(sys) = system {
        body["system"] = serde_json::Value::String(sys);
    }
    body
}
