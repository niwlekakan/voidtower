/// Local LLM provider — targets Ollama or llama.cpp via OpenAI-compatible API.
use crate::ai::{AiCapabilities, AiProvider, AiRequest};
use async_trait::async_trait;

pub struct LocalLlmProvider {
    id: String,
    #[allow(dead_code)]
    name: String,
    base_url: String,
    model: String,
}

impl LocalLlmProvider {
    pub fn new(id: String, name: String, base_url: String, model: String) -> Self {
        Self { id, name, base_url, model }
    }

    fn completions_url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn tags_url(&self) -> String {
        format!("{}/api/tags", self.base_url.trim_end_matches('/'))
    }
}

#[async_trait]
impl AiProvider for LocalLlmProvider {
    fn id(&self) -> &str { &self.id }
    fn display_name(&self) -> &str { &self.name }

    fn capabilities(&self) -> AiCapabilities {
        AiCapabilities {
            reasoning: 5,
            coding: 6,
            tool_use: false,
            vision: false,
            local: true,
            streaming: true,
        }
    }

    async fn complete(&self, req: &AiRequest) -> std::result::Result<String, String> {
        let body = build_body(req, &self.model, false);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build().map_err(|e| e.to_string())?;

        let resp = client
            .post(self.completions_url())
            .json(&body)
            .send().await
            .map_err(|e| format!("Local LLM unreachable at {}: {e}", self.base_url))?;

        if !resp.status().is_success() {
            return Err(format!("Local LLM error: HTTP {}", resp.status()));
        }
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        json.get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Unexpected local LLM response shape".to_string())
    }

    async fn stream(&self, req: &AiRequest) -> std::result::Result<reqwest::Response, String> {
        let body = build_body(req, &self.model, true);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build().map_err(|e| e.to_string())?;

        client
            .post(self.completions_url())
            .json(&body)
            .send().await
            .map_err(|e| format!("Local LLM unreachable: {e}"))
    }

    async fn health_check(&self) -> std::result::Result<(), String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build().map_err(|e| e.to_string())?;

        if client.get(self.tags_url()).send().await.map(|r| r.status().is_success()).unwrap_or(false) {
            return Ok(());
        }
        let models_url = format!("{}/v1/models", self.base_url.trim_end_matches('/'));
        let resp = client.get(&models_url).send().await.map_err(|e| e.to_string())?;
        if resp.status().is_success() { Ok(()) }
        else { Err(format!("HTTP {}", resp.status())) }
    }
}

fn build_body(req: &AiRequest, model: &str, stream: bool) -> serde_json::Value {
    let mut messages: Vec<serde_json::Value> = Vec::new();
    if let Some(sys) = &req.system_prompt {
        messages.push(serde_json::json!({ "role": "system", "content": sys }));
    }
    for m in &req.messages {
        messages.push(serde_json::json!({ "role": m.role, "content": m.content }));
    }
    serde_json::json!({ "model": model, "messages": messages, "stream": stream })
}
