use crate::ai::{egress, AiCapabilities, AiProvider, AiRequest};
use async_trait::async_trait;

pub struct OpenAiProvider {
    id: String,
    #[allow(dead_code)]
    name: String,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(id: String, name: String, base_url: String, api_key: String, model: String) -> Self {
        Self { id, name, base_url, api_key, model }
    }

    fn completions_url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/'))
    }

    async fn client(&self, timeout: std::time::Duration) -> std::result::Result<reqwest::Client, String> {
        egress::client_for(&self.base_url, timeout).await
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn id(&self) -> &str { &self.id }
    fn display_name(&self) -> &str { &self.name }

    fn capabilities(&self) -> AiCapabilities {
        AiCapabilities {
            reasoning: 9,
            coding: 9,
            tool_use: true,
            vision: true,
            local: false,
            streaming: true,
        }
    }

    async fn complete(&self, req: &AiRequest) -> std::result::Result<String, String> {
        let body = build_body(req, &self.model, false);
        let resp = self.client(std::time::Duration::from_secs(120)).await?
            .post(self.completions_url())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send().await
            .map_err(|e| format!("OpenAI unreachable: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(format!("OpenAI request failed with HTTP {status}"));
        }
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        extract_content(&json)
    }

    async fn stream(&self, req: &AiRequest) -> std::result::Result<reqwest::Response, String> {
        let body = build_body(req, &self.model, true);
        let client = self.client(std::time::Duration::from_secs(300)).await?;

        let resp = client
            .post(self.completions_url())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send().await
            .map_err(|e| format!("OpenAI unreachable: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("OpenAI request failed with HTTP {}", resp.status()));
        }
        Ok(resp)
    }

    async fn health_check(&self) -> std::result::Result<(), String> {
        let resp = self.client(std::time::Duration::from_secs(5)).await?
            .get(format!("{}/v1/models", self.base_url.trim_end_matches('/')))
            .bearer_auth(&self.api_key)

            .send().await
            .map_err(|e| e.to_string())?;
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

fn extract_content(json: &serde_json::Value) -> std::result::Result<String, String> {
    json.get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Unexpected response shape".to_string())
}
