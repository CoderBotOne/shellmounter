#![allow(dead_code)]
use super::{AiProvider, ProviderMessage, StreamChunk, TokenUsage, ToolDef};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde_json::Value;
use tokio::sync::mpsc;

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".into()),
        }
    }

    /// Create a provider for any OpenAI-compatible endpoint (Ollama, LM Studio, etc.)
    pub fn new_compatible(api_key: String, base_url: String) -> Self {
        Self::new(api_key, Some(base_url))
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ProviderMessage],
        system_prompt: Option<&str>,
        tools: &[ToolDef],
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        let mut body = serde_json::json!({
            "model": model,
            "messages": build_messages(system_prompt, messages),
            "stream": true,
            "stream_options": {"include_usage": true},
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::to_value(tools)?;
            body["tool_choice"] = serde_json::json!("auto");
        }

        let (tx, rx) = mpsc::channel(256);

        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body_val = body;

        tokio::spawn(async move {
            let result = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&body_val)
                .send()
                .await;

            let response = match result {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(StreamChunk::Error(format!("HTTP error: {e}"))).await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let _ = tx
                    .send(StreamChunk::Error(format!(
                        "API error {status}: {body}"
                    )))
                    .await;
                return;
            }

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut current_tool_id: Option<String> = None;
            let mut input_tokens: u64 = 0;
            let mut output_tokens: u64 = 0;

            while let Some(chunk) = stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(StreamChunk::Error(format!("Stream error: {e}"))).await;
                        return;
                    }
                };
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim().to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    let data = match line.strip_prefix("data: ") {
                        Some(d) => d,
                        None => continue,
                    };

                    if data == "[DONE]" {
                        let _ = tx
                            .send(StreamChunk::Done(TokenUsage {
                                input_tokens,
                                output_tokens,
                                cached_input_tokens: 0,
                            }))
                            .await;
                        return;
                    }

                    let parsed: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Extract usage info
                    if let Some(usage) = parsed.get("usage") {
                        if let Some(it) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                            input_tokens = it;
                        }
                        if let Some(ot) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
                            output_tokens = ot;
                        }
                    }

                    let choices = match parsed.get("choices") {
                        Some(c) => c,
                        None => continue,
                    };

                    let choice = match choices.get(0) {
                        Some(c) => c,
                        None => continue,
                    };

                    let delta = match choice.get("delta") {
                        Some(d) => d,
                        None => continue,
                    };

                    // Content delta
                    if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                        if !content.is_empty() {
                            let _ = tx
                                .send(StreamChunk::TextDelta(content.to_string()))
                                .await;
                        }
                    }

                    // Reasoning (for DeepSeek R1, o1, etc.)
                    if let Some(reasoning) =
                        delta.get("reasoning_content").and_then(|v| v.as_str())
                    {
                        if !reasoning.is_empty() {
                            let _ = tx
                                .send(StreamChunk::ReasoningDelta(reasoning.to_string()))
                                .await;
                        }
                    }

                    // Tool calls
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                        for tc in tool_calls {
                            let id = tc
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let name = tc
                                .get("function")
                                .and_then(|f| f.get("name"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");

                            if let Some(args) = tc
                                .get("function")
                                .and_then(|f| f.get("arguments"))
                                .and_then(|v| v.as_str())
                            {
                                let _ = tx
                                    .send(StreamChunk::ToolCallArgs(args.to_string()))
                                    .await;
                            } else if !name.is_empty() {
                                current_tool_id = Some(id.to_string());
                                let _ = tx
                                    .send(StreamChunk::ToolCallStart {
                                        id: id.to_string(),
                                        name: name.to_string(),
                                    })
                                    .await;
                            }

                            // Tool call end (when an index is present with empty function)
                            if current_tool_id.is_some() {
                                let _ = tx
                                    .send(StreamChunk::ToolCallEnd {
                                        id: current_tool_id.take().unwrap(),
                                    })
                                    .await;
                            }
                        }
                    }

                    // Finish reason
                    if choice.get("finish_reason").and_then(|v| v.as_str()) == Some("tool_calls") {
                        if let Some(id) = current_tool_id.take() {
                            let _ = tx.send(StreamChunk::ToolCallEnd { id }).await;
                        }
                    }
                }
            }

            let _ = tx
                .send(StreamChunk::Done(TokenUsage {
                    input_tokens,
                    output_tokens,
                    cached_input_tokens: 0,
                }))
                .await;
        });

        Ok(rx)
    }

    async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to list models")?;

        let json: Value = response.json().await?;
        let models = json["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }
}

fn build_messages(system: Option<&str>, messages: &[ProviderMessage]) -> Vec<Value> {
    let mut msgs: Vec<Value> = Vec::new();

    if let Some(sys) = system {
        msgs.push(serde_json::json!({
            "role": "system",
            "content": sys,
        }));
    }

    for msg in messages {
        let content = match &msg.content {
            super::ProviderContent::Text(t) => serde_json::json!(t),
            super::ProviderContent::Parts(parts) => {
                let parts_json: Vec<Value> = parts
                    .iter()
                    .map(|p| match p {
                        super::ContentPart::Text { text } => {
                            serde_json::json!({"type": "text", "text": text})
                        }
                        super::ContentPart::ToolResult {
                            tool_call_id,
                            content,
                        } => serde_json::json!({
                            "type": "tool_result",
                            "tool_call_id": tool_call_id,
                            "content": content,
                        }),
                    })
                    .collect();
                serde_json::json!(parts_json)
            }
        };

        msgs.push(serde_json::json!({
            "role": msg.role,
            "content": content,
        }));
    }

    msgs
}
